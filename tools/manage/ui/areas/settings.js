/* Settings: each program's own file, shown masked and never written. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  function program(entry) {
    if (entry.error) return ui.card(null, ui.empty("These settings could not be read", entry.error));
    var table = ui.table({
      columns: [
        { label: "Setting", sort: function (row) { return row.key.toLowerCase(); }, value: function (row) { return h("b", { text: row.key }); } },
        { label: "Value", value: function (row) { return row.value === "" ? h("span", { class: "muted", text: "(empty)" }) : h("code", { text: row.value }); } }
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
    hint: "what each program's file says",
    render: async function (view) {
      var data = await api.get("/settings");
      view.body(
        ui.banner("Settings are shown, not edited. Change the file and restart the service; passwords are masked. A value is the file's: an environment variable can override it when the program starts."),
        ui.tabs(data.programs.map(function (entry) {
          return { id: entry.id, label: entry.label, render: function () { return program(entry); } };
        })));
    }
  });
})();
