/* Content: the world pack - maps, the creation lists, running copies, the world list, titles. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api, fmt = Kryta.fmt;

  function point(value) { return Number(value).toFixed(2); }

  function mapDrawer(map) {
    ui.drawer(map.name || "Map " + map.id, [
      ui.kv([
        ["Map id", String(map.id)],
        ["Kind", map.kind],
        ["Campaign folder", map.campaign || "not in the pack's folders"],
        ["Archive file id", h("code", { text: map.file_id })],
        ["Hosted", map.hostable ? "yes" : "no - the client can name it, this world cannot load it"]
      ]),
      h("h3", { text: "Entry points (" + map.spawns.length + ")" }),
      map.spawns.length ? ui.table({
        columns: [
          { label: "x", num: true, value: function (row) { return point(row.x); } },
          { label: "y", num: true, value: function (row) { return point(row.y); } },
          { label: "Plane", num: true, value: function (row) { return row.plane; } },
          { label: "Facing", num: true, value: function (row) { return row.angle === null ? "-" : point(row.angle); } }
        ], rows: map.spawns
      }) : h("p", { class: "muted", text: "None measured: a character has no stated place to stand on entry." }),
      h("h3", { text: "Arrivals (" + map.arrivals.length + ")" }),
      map.arrivals.length ? ui.table({
        columns: [
          { label: "From map", num: true, value: function (row) { return row.from; } },
          { label: "x", num: true, value: function (row) { return point(row.x); } },
          { label: "y", num: true, value: function (row) { return point(row.y); } },
          { label: "Plane", num: true, value: function (row) { return row.plane; } }
        ], rows: map.arrivals
      }) : h("p", { class: "muted", text: "No doorway lands here." })
    ]);
  }

  async function maps() {
    var data = await api.get("/content/maps");
    var typed = "", campaign = "all", kind = "all";
    var unique = function (key) {
      var seen = {};
      data.maps.forEach(function (map) { if (map[key]) seen[map[key]] = true; });
      return Object.keys(seen).sort();
    };
    var table = ui.table({
      columns: [
        { label: "Id", num: true, sort: function (row) { return row.id; }, value: function (row) { return row.id; } },
        { label: "Name", sort: function (row) { return row.name.toLowerCase(); }, value: function (row) { return h("b", { text: row.name || "(unnamed)" }); } },
        { label: "Campaign", sort: function (row) { return row.campaign || ""; }, value: function (row) { return row.campaign || "-"; } },
        { label: "Kind", sort: function (row) { return row.kind; }, value: function (row) { return row.kind; } },
        { label: "Entry points", num: true, sort: function (row) { return row.spawns.length; }, value: function (row) { return row.spawns.length; } },
        { label: "Arrivals", num: true, sort: function (row) { return row.arrivals.length; }, value: function (row) { return row.arrivals.length; } },
        { label: "Hosted", value: function (row) { return ui.badge(row.hostable ? "yes" : "no", row.hostable ? "ok" : "plain"); } }
      ],
      onRow: mapDrawer,
      empty: { title: "No map matches", text: "Change the search or the filters." }
    });
    var summary = h("span", { class: "muted small" });
    function filter() {
      var rows = data.maps.filter(function (map) {
        return (campaign === "all" || map.campaign === campaign) && (kind === "all" || map.kind === kind) &&
          (!typed || ui.matches(typed, [map.name, String(map.id)]));
      });
      summary.textContent = fmt.plural(rows.length, "map") + " of " + fmt.number(data.maps.length) + " in " + data.pack;
      table.update(rows);
    }
    filter();
    return [
      ui.toolbar(
        ui.search("Search maps by name or id", function (value) { typed = value; filter(); }),
        ui.select([{ value: "all", label: "Every campaign" }].concat(unique("campaign")), function (value) { campaign = value; filter(); }, campaign, "Campaign"),
        ui.select([{ value: "all", label: "Every kind" }].concat(unique("kind")), function (value) { kind = value; filter(); }, kind, "Kind"),
        summary),
      ui.card(null, table, { flush: true })
    ];
  }

  async function creation() {
    var data = await api.get("/content/creation");
    return h("div", { class: "stack" },
      ui.card(["Profession sets", h("span", { class: "muted", text: data.sets.length + " a creation screen may choose from" })], ui.table({
        columns: [
          { label: "Campaign", value: function (row) { return row.campaign; } },
          { label: "Profession", value: function (row) { return row.profession; } },
          { label: "Items", num: true, value: function (row) { return row.items; } },
          { label: "Archive file ids", value: function (row) { return h("code", { text: row.file_ids.join(", ") }); } }
        ], rows: data.sets, empty: { title: "The creation lists hold no set" }
      }), { flush: true }),
      ui.card(["Item definitions", h("span", { class: "muted", text: data.items.length + " from the creation lists" })], ui.table({
        columns: [
          { label: "Name", sort: function (row) { return row.name.toLowerCase(); }, value: function (row) { return ui.itemName(row.name); } },
          { label: "File id", num: true, value: function (row) { return h("code", { text: row.file_id }); } },
          { label: "Type", num: true, sort: function (row) { return row.item_type; }, value: function (row) { return row.item_type; } },
          { label: "Dyes", value: function (row) { return row.dyes; } },
          { label: "Model", num: true, value: function (row) { return row.model_id; } },
          { label: "Value", num: true, value: function (row) { return row.value; } },
          { label: "Profession", value: function (row) { return row.profession; } },
          { label: "Stack", num: true, value: function (row) { return row.quantity; } }
        ], rows: data.items, empty: { title: "The creation lists hold no item" }
      }), { flush: true }));
  }

  async function instances() {
    var data = await api.get("/content/instances");
    return ui.card(null, ui.table({
      columns: [
        { label: "Map", value: function (row) { return [h("b", { text: row.map || "map " + row.map_id }), h("div", { class: "sub", text: String(row.map_id) })]; } },
        { label: "District", num: true, value: function (row) { return row.district; } },
        { label: "Language", num: true, value: function (row) { return row.language; } },
        { label: "Kind", value: function (row) { return row.kind; } },
        { label: "Opened", value: function (row) { return row.opened_at || "-"; } },
        { label: "Empty since", value: function (row) { return row.empty_since || ui.badge("somebody inside", "ok", "live"); } }
      ],
      rows: data.instances,
      empty: data.configured
        ? { title: "No copy is open", text: "The world writes a row here when it opens a copy of a map, and removes it once the copy has stayed empty." }
        : { title: "No world database", text: "WorldDatabaseInfo is empty in manage.json, so the instance table cannot be read." }
    }), { flush: true });
  }

  async function worlds() {
    var data = await api.get("/content/worlds");
    return [ui.card(null, ui.table({
      columns: [
        { label: "Id", num: true, value: function (row) { return row.id; } },
        { label: "Name", value: function (row) { return h("b", { text: row.name }); } },
        { label: "Public address", value: function (row) { return h("code", { text: row.address }); } },
        { label: "Local address", value: function (row) { return h("code", { text: row.local_address }); } }
      ],
      rows: data.worlds,
      empty: { title: "The world list is empty", text: "Add a world to data/realms.json and restart the login program and this panel." }
    }), { flush: true }),
    h("p", { class: "muted small" }, h("br"), "Read when the panel started: data/realms.json merged over the realmlist table. The file row wins when both name a world.")];
  }

  async function titles() {
    var data = await api.get("/content/titles");
    return h("div", { class: "stack" },
      ui.banner("One sheet, the same for every player: no metric is recorded yet, so each title shows the rank zero gives it.", "info"),
      ui.card(["Titles", h("span", { class: "muted", text: data.titles.length + " rows in the pack's titles.json" })], ui.table({
        columns: [
          { label: "Title", sort: function (row) { return row.name.toLowerCase(); }, value: function (row) { return h("b", { text: row.name }); } },
          { label: "Metric", value: function (row) { return h("code", { text: row.metric }); } },
          { label: "Points", num: true, value: function (row) { return row.points; } },
          { label: "Rank", num: true, value: function (row) { return row.rank; } },
          { label: "Next rank at", num: true, value: function (row) { return fmt.number(row.next); } }
        ], rows: data.titles, empty: { title: "The pack names no title" }
      }), { flush: true }),
      ui.card(["Metrics", h("span", { class: "muted", text: data.metrics.length + " the platform keeps" })], ui.table({
        columns: [
          { label: "Metric", value: function (row) { return h("code", { text: row.key }); } },
          { label: "Label", value: function (row) { return row.label; } },
          { label: "Value", num: true, value: function (row) { return row.value; } },
          { label: "Unit", value: function (row) { return row.unit; } }
        ], rows: data.metrics, empty: { title: "No metric is defined" }
      }), { flush: true }));
  }

  Kryta.area({
    id: "content",
    label: "Content",
    group: "World",
    hint: "the world pack, read from the files the world reads at start-up",
    find: async function () {
      return (await api.get("/content/maps")).maps.filter(function (map) { return map.name; }).map(function (map) {
        return { label: map.name, hint: "map " + map.id, href: "#/content/maps?find=" + map.id };
      });
    },
    render: async function (view, route) {
      var tabs = [
        { id: "maps", label: "Maps", render: maps },
        { id: "creation", label: "Creation", render: creation },
        { id: "titles", label: "Titles", render: titles },
        { id: "instances", label: "Running copies", render: instances },
        { id: "worlds", label: "World list", render: worlds }
      ];
      view.body(
        ui.banner("Content is shown, not edited. Change the pack's JSON files and restart the world."),
        ui.tabs(tabs, route.parts[0] || "maps", function (id) {
          window.history.replaceState(null, "", "#/content/" + id);
        }));
      if (route.query.find) {
        var found = (await api.get("/content/maps")).maps.filter(function (map) { return String(map.id) === route.query.find; })[0];
        if (found && view.alive()) mapDrawer(found);
      }
    }
  });
})();
