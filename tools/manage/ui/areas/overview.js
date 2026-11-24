/* Overview: the counts, the stack, the running world and what needs attention. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api, fmt = Kryta.fmt;

  function servicesCard(target) {
    return api.get("/services").then(function (data) {
      Kryta.clear(target);
      if (!data.available) {
        target.appendChild(ui.empty("The supervisor is not answering",
          "Start the stack with: cargo run -p kryta-supervisor", ui.link("Open services", "#/services", "small")));
        return;
      }
      target.appendChild(ui.table({
        columns: [
          { label: "Service", value: function (row) { return h("b", { text: row.name }); } },
          { label: "State", value: function (row) { return ui.badge(row.state, row.up ? "ok" : "warn", row.up ? "live" : true); } },
          { label: "Port", num: true, value: function (row) { return row.port || "-"; } }
        ],
        rows: data.services,
        empty: { title: "The supervisor runs no service" }
      }));
    });
  }

  function worldCard(target) {
    return api.get("/console/status").then(function (data) {
      Kryta.clear(target);
      if (data.available) {
        target.appendChild(ui.check(ui.badge("answering", "ok", "live"), "The world answered world.status"));
        target.appendChild(ui.log(data.result || "(the world answered, and its result is empty)"));
      } else {
        target.appendChild(ui.check(ui.badge(data.configured ? "no answer" : "not configured", data.configured ? "bad" : "plain", true),
          data.configured ? "The running world did not answer" : "No world control endpoint is configured", data.error));
      }
    });
  }

  Kryta.area({
    id: "overview",
    label: "Overview",
    group: "Server",
    hint: "this machine's server at a glance",
    render: async function (view) {
      var data = await api.get("/overview");
      var counts = data.counts;
      var services = h("div", null, ui.skeleton(2));
      var world = h("div", { class: "body" }, ui.skeleton(2));
      var live = function () {
        servicesCard(services).catch(function () {});
        worldCard(world).catch(function () {});
      };
      view.body(
        ui.grid("g4",
          ui.stat("Accounts", fmt.number(counts.accounts), fmt.number(counts.active_accounts) + " active"),
          ui.stat("Characters", fmt.number(counts.characters), counts.characters === null ? "no character database" : "across every account"),
          ui.stat("Maps in the pack", fmt.number(counts.maps), counts.maps === null ? "the world pack could not be read" : "read from the world's own files"),
          ui.stat("Needs attention", data.attention.filter(function (item) { return item.level !== "ok"; }).length, "of " + data.attention.length + " checks")),
        h("div", { class: "stack" }, h("span"),
          ui.grid("g3",
            h("div", { class: "stack" },
              ui.card(["Services", h("span", { class: "spacer" }), ui.link("Manage", "#/services", "small")], services, { flush: true }),
              ui.card(["Running world", h("span", { class: "spacer" }), ui.link("Console", "#/console", "small")], world, { flush: true })),
            ui.card("Needs attention", data.attention.map(function (item) {
              return ui.check(ui.badge(item.level === "ok" ? "ok" : item.level === "bad" ? "fix" : "note", item.level, true),
                item.text, item.detail, item.href);
            })))));
      live();
      view.every(5000, live);
    }
  });
})();
