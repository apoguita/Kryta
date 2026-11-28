/* Settings: each program's settings in the flavor this machine runs, shown masked and changed one
   value at a time, in the file that states the value: the flavor's or the machine's. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  /* The file a row's value is in, or would be written into. */
  function fileOf(entry, row) {
    return row.source === "machine" ? entry.machine_path : entry.path;
  }

  function change(entry, row, view) {
    var field = { name: "value", label: row.key, full: true, hint: row.doc || null };
    if (row.kind === "flag") {
      field.type = "checkbox";
      field.value = row.value === "1" || row.value === "true";
    } else {
      field.type = row.kind === "number" ? "number" : "text";
      field.value = row.value;
    }
    var where = row.source === "machine"
      ? "This is this machine's own value, the same for every flavor it runs. "
      : "This is the " + entry.flavor + " flavor's value, and it travels with the flavor. ";
    ui.formModal({
      title: "Change " + row.key,
      intro: where + (row.in_file ? "Written into " : "Not in the file yet; saving adds it to ") + fileOf(entry, row) +
        ". The program reads its settings when it starts, so restart the service afterwards.",
      submit: "Save",
      fields: [field],
      onSubmit: async function (values) {
        var saved = await api.post("/settings/" + entry.id + "/set", { key: row.key, value: values.value });
        ui.toast(row.key + " saved in " + saved.path + ". Restart the service for it to take effect.");
        view.reload();
      }
    });
  }

  function program(entry, view) {
    if (entry.error) return ui.card(null, ui.empty("These settings could not be read", entry.error));
    var typed = "", source = "all";
    var table = ui.table({
      columns: [
        { label: "Setting", sort: function (row) { return row.key.toLowerCase(); }, value: function (row) {
          return [h("b", { text: row.key }), row.in_file ? null : " ", row.in_file ? null : ui.badge("default, not in the file")];
        } },
        { label: "Value", value: function (row) { return row.value === "" ? h("span", { class: "muted", text: "(empty)" }) : h("code", { text: row.value }); } },
        { label: "Stated by", sort: function (row) { return row.source; }, value: function (row) {
          return row.source === "machine"
            ? [ui.badge("this machine", "warn"), row.overrides_flavor ? " over the flavor's" : null]
            : ui.badge(row.source === "flavor" ? "flavor " + entry.flavor : "file");
        } },
        { label: "", value: function (row) {
          return row.editable ? ui.button("Change", function () { change(entry, row, view); }, "small") : null;
        } }
      ],
      rows: entry.rows,
      empty: { title: "No setting matches" }
    });
    function filter() {
      table.update(entry.rows.filter(function (row) {
        return (source === "all" || row.source === source) && (!typed || ui.matches(typed, [row.key, row.value]));
      }));
    }
    return [
      ui.toolbar(
        ui.search("Search settings", function (text) { typed = text; filter(); }),
        ui.select([
          { value: "all", label: "Flavor and machine" },
          { value: "flavor", label: "The flavor's only" },
          { value: "machine", label: "This machine's only" }
        ], function (value) { source = value; filter(); }, source, "Which file states the setting"),
        h("span", { class: "muted small", text: entry.rows.length + " keys - flavor: " + entry.path + " - machine: " + entry.machine_path })),
      ui.card(null, table, { flush: true })
    ];
  }

  Kryta.area({
    id: "settings",
    label: "Settings",
    group: "World",
    hint: "each program's settings in the running flavor, and this machine's own",
    render: async function (view) {
      var data = await api.get("/settings");
      var flavor = (data.programs[0] && data.programs[0].flavor) || null;
      view.body(
        ui.banner((flavor ? "These are the settings of the " + flavor + " flavor, the one this machine runs (Flavors), with this machine's own values over them. " : "") +
          "A flavor's value is written into the flavor's file and is shared with the flavor; this machine's value (addresses, ports, database logins) is written into machine.json and never leaves this machine. " +
          "A change takes effect when the service is restarted (Services). Passwords are masked and are changed in the file, as are lists. An environment variable can override a value when the program starts."),
        ui.tabs(data.programs.map(function (entry) {
          return { id: entry.id, label: entry.label, render: function () { return program(entry, view); } };
        })));
    }
  });
})();
