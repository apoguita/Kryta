/* Flavors: the server setups this machine holds, the one it runs, and making another. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  var RESTART = "The programs read the flavor when they start: restart the stack (Services) to run it.";

  async function activate(row, view) {
    var sure = await ui.confirm({
      title: "Run " + row.name + " on this machine?",
      text: "The name is written into flavor.json. Nothing running changes now: the login and world programs read it at their next start, and from then on they serve " +
        row.name + "'s settings, world and client files. This machine's own values (machine.json) stay as they are.",
      label: "Make it the flavor"
    });
    if (!sure) return;
    await ui.act(function () { return api.post("/flavors/activate", { name: row.name }); },
      function (data) { return data.active + " is now the flavor this machine runs. " + RESTART; });
    view.reload();
  }

  function copy(data, from, view) {
    ui.formModal({
      title: "New flavor",
      intro: "A flavor is made as a whole copy of another one: its two settings files, its world, its command surface and its client files. Change the copy afterwards. " +
        "The name is a folder on this machine and on every client that connects, so it is 1 to 64 letters, digits, '_' and '-'.",
      submit: "Make the flavor",
      fields: [
        { name: "name", label: "Name", type: "text", placeholder: "fantasy_modded", full: true },
        { name: "from", label: "Copy of", type: "select", value: from,
          options: data.flavors.map(function (flavor) { return flavor.name; }), full: true }
      ],
      onSubmit: async function (values) {
        var made = await api.post("/flavors/create", { name: values.name, from: values.from });
        ui.toast(made.name + " was made in " + made.path + " (" + made.files + " files). It is not the running flavor until you choose it.");
        view.reload();
      }
    });
  }

  Kryta.area({
    id: "flavors",
    label: "Flavors",
    group: "Server",
    hint: "the server setups on this machine, and which one it runs",
    render: async function (view) {
      var data = await api.get("/flavors");
      var active = data.active || (data.flavors[0] && data.flavors[0].name);

      view.head({ actions: data.flavors.length
        ? ui.button("New flavor", function () { copy(data, active, view); }, "primary") : null });

      var table = ui.table({
        columns: [
          { label: "Flavor", value: function (row) {
            return [h("b", { text: row.name }), " ", row.active ? ui.badge("this machine runs it", "ok") : null];
          } },
          { label: "Holds", value: function (row) {
            var parts = row.holds.map(function (part) { return ui.badge(part); });
            row.lacks.forEach(function (file) { parts.push(ui.badge("no " + file, "warn")); });
            return parts.length ? parts.reduce(function (all, part) { all.push(part, " "); return all; }, []) : h("span", { class: "muted", text: "nothing" });
          } },
          { label: "Maps", num: true, value: function (row) { return row.maps; } },
          { label: "Folder", value: function (row) { return h("code", { text: row.path }); } },
          { label: "", class: "actions", value: function (row) {
            return [
              row.active || row.lacks.length ? null : ui.button("Run this flavor", function () { activate(row, view); }, "small"),
              ui.button("Copy", function () { copy(data, row.name, view); }, "small")
            ];
          } }
        ],
        rows: data.flavors,
        empty: { title: "This machine holds no flavor", text: "A flavor is a folder under " + data.config_folder + "/flavors/." }
      });

      view.body(
        data.error ? ui.banner("The flavor this machine runs could not be read: " + data.error + " Choose one below.", "warn") : null,
        ui.banner("A flavor is one server's whole setup: the login and world programs' settings, the world's content, the command surface and the files a patched client is given. " +
          "This machine runs the one that " + data.active_file + " names. " + RESTART +
          " Settings, Content and the other pages show the flavor named there. Addresses, ports and database logins are this machine's own and are the same for every flavor (Settings)."),
        ui.card(null, table, { flush: true }),
        h("p", { class: "muted small" }, h("br"),
          "A flavor is not renamed or removed from here: its name is also a folder on every client that connected to it. Do that in the files, with the stack stopped."));
    }
  });
})();
