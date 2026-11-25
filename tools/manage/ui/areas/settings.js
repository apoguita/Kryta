/* Settings: each program's own file, shown masked and changed one value at a time. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  function change(entry, row, view) {
    var field = { name: "value", label: row.key, full: true, hint: row.doc || null };
    if (row.kind === "flag") {
      field.type = "checkbox";
      field.value = row.value === "1" || row.value === "true";
    } else {
      field.type = row.kind === "number" ? "number" : "text";
      field.value = row.value;
    }
    ui.formModal({
      title: "Change " + row.key,
      intro: (row.in_file ? "Written into " : "Not in the file yet; saving adds it to ") + entry.path +
        ". The program reads its file when it starts, so restart the service afterwards.",
      submit: "Save",
      fields: [field],
      onSubmit: async function (values) {
        await api.post("/settings/" + entry.id + "/set", { key: row.key, value: values.value });
        ui.toast(row.key + " saved. Restart the service for it to take effect.");
        view.reload();
      }
    });
  }

  function program(entry, view) {
    if (entry.error) return ui.card(null, ui.empty("These settings could not be read", entry.error));
    var table = ui.table({
      columns: [
        { label: "Setting", sort: function (row) { return row.key.toLowerCase(); }, value: function (row) {
          return [h("b", { text: row.key }), row.in_file ? null : " ", row.in_file ? null : ui.badge("default, not in the file")];
        } },
        { label: "Value", value: function (row) { return row.value === "" ? h("span", { class: "muted", text: "(empty)" }) : h("code", { text: row.value }); } },
        { label: "", value: function (row) {
          return row.editable ? ui.button("Change", function () { change(entry, row, view); }, "small") : null;
        } }
      ],
      rows: entry.rows,
      empty: { title: "No setting matches" }
    });
    return [
      ui.toolbar(
        ui.search("Search settings", function (typed) {
          table.update(entry.rows.filter(function (row) { return !typed || ui.matches(typed, [row.key, row.value]); }));
        }),
        h("span", { class: "muted small", text: entry.path + " - " + entry.rows.length + " keys" })),
      ui.card(null, table, { flush: true })
    ];
  }

  Kryta.area({
    id: "settings",
    label: "Settings",
    group: "World",
    hint: "what each program's file says, and changing it",
    render: async function (view) {
      var data = await api.get("/settings");
      view.body(
        ui.banner("A change is written into the program's file and takes effect when the service is restarted (Services). Passwords are masked and are changed in the file, as are lists. A value is the file's: an environment variable can override it when the program starts."),
        ui.tabs(data.programs.map(function (entry) {
          return { id: entry.id, label: entry.label, render: function () { return program(entry, view); } };
        })));
    }
  });
})();
