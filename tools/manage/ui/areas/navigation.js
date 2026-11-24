/* Navigation: the extracted path-graph cache - inspect it, re-dump one map. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api, fmt = Kryta.fmt;

  Kryta.area({
    id: "navigation",
    label: "Navigation",
    group: "World",
    hint: "the path-graph cache the world reads",
    render: async function (view) {
      var data = await api.get("/navigation");
      var typed = "";
      var table = ui.table({
        columns: [
          { label: "Map id", num: true, sort: function (row) { return row.map_id; },
            value: function (row) { return row.map_id < 0 ? ui.badge("unresolved", "warn") : row.map_id; } },
          { label: "Map", sort: function (row) { return (row.name || "").toLowerCase(); },
            value: function (row) { return row.name || h("span", { class: "muted", text: "unknown map" }); } },
          { label: "Archive file id", num: true, value: function (row) { return h("code", { text: row.file_id }); } },
          { label: "Layers", num: true, sort: function (row) { return row.planes; }, value: function (row) { return row.planes; } },
          { label: "Walkable areas", num: true, sort: function (row) { return row.trapezoids; }, value: function (row) { return fmt.number(row.trapezoids); } }
        ],
        empty: { title: "No graph matches", text: "A map the cache omits has no graph; dump it in below." }
      });
      function filter() {
        table.update(data.maps.filter(function (row) {
          return !typed || ui.matches(typed, [row.name, String(row.map_id)]);
        }));
      }
      filter();

      var dump = ui.form({
        submit: "Dump map into the cache",
        fields: [{ name: "map_id", label: "Map id", type: "number", min: 0, placeholder: "148", full: true,
          hint: "Replaces only this map's graph, read from the client archive MapArchiveFile names." }],
        onSubmit: async function (values) {
          var result = await api.post("/navigation/dump", values);
          ui.toast("Map " + result.map_id + " dumped: " + result.graph_count + " graphs, " + fmt.bytes(result.bytes) + ".");
          view.reload();
        }
      });

      view.head({ sub: data.path + " - " + fmt.bytes(data.bytes) + ", " + fmt.plural(data.maps.length, "graph") });
      view.body(ui.grid("g3",
        h("div", null,
          ui.toolbar(ui.search("Search by map name or id", function (value) { typed = value; filter(); })),
          ui.card(null, table, { flush: true })),
        h("div", { class: "stack" },
          ui.card("Dump one map", [
            data.archive_configured ? null : ui.banner("MapArchiveFile is empty in manage.json, so there is no archive to read from. Set it and restart the panel."),
            dump]),
          ui.card("What this is", h("p", { class: "muted", text:
            "The world reads this cache and never a client archive. A cache may omit maps; an omitted map has no graph and nothing can path on it." })))));
    }
  });
})();
