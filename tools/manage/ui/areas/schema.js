/* Schema: what each database recorded as applied, against the files on disk. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;
  var KINDS = { "applied": "ok", "new": "warn", "changed": "bad", "recorded, not on disk": "warn" };

  Kryta.area({
    id: "schema",
    label: "Schema",
    group: "World",
    hint: "the updater's record against the files on disk",
    render: async function (view) {
      var data = await api.get("/schema");
      view.body(h("div", { class: "stack" },
        data.databases.map(function (database) {
          var odd = database.files.filter(function (file) { return file.state !== "applied"; }).length;
          return ui.card([database.label, h("span", { class: "muted", text: database.files.length + " files" }),
            h("span", { class: "spacer" }), ui.badge(odd ? odd + " to look at" : "current", odd ? "warn" : "ok", true)],
            ui.table({
              columns: [
                { label: "File", value: function (row) { return h("code", { text: row.name }); } },
                { label: "State", value: function (row) { return ui.badge(row.state, KINDS[row.state] || "plain", true); } }
              ],
              rows: database.files,
              empty: { title: "No schema file", text: "Neither the database nor the folder names one." }
            }), { flush: true });
        }),
        h("p", { class: "muted small", text:
          "new: not applied yet. changed: edited after it was applied, which the updater refuses to apply again. recorded, not on disk: the database holds a record for a file this checkout does not carry." })));
    }
  });
})();
