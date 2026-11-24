/* Services: the supervisor's stack - state, restart, stop. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  Kryta.area({
    id: "services",
    label: "Services",
    group: "Server",
    hint: "the running stack, through the supervisor",
    render: async function (view) {
      var holder = h("div", null, ui.skeleton(3));
      var stamp = h("span", { class: "muted small" });

      async function act(verb, name) {
        var sure = await ui.confirm({
          title: verb === "restart" ? "Restart " + name + "?" : "Stop " + name + "?",
          text: verb === "restart"
            ? "The supervisor stops " + name + " and starts it again. Anyone connected to it is disconnected."
            : "The supervisor stops " + name + " and leaves it stopped. Restart starts it again; the supervisor has no separate start.",
          label: verb === "restart" ? "Restart" : "Stop",
          danger: verb !== "restart"
        });
        if (!sure) return;
        await ui.act(function () { return api.post("/services/" + encodeURIComponent(name) + "/" + verb); },
          function (data) { return "Supervisor: " + data.answer; });
        refresh();
      }

      async function refresh() {
        var data;
        try { data = await api.get("/services"); } catch (error) { data = { available: false, error: error.message, services: [] }; }
        if (!view.alive()) return;
        Kryta.clear(holder);
        stamp.textContent = "checked " + new Date().toLocaleTimeString();
        if (!data.available) {
          holder.appendChild(ui.card(null, ui.empty("The supervisor is not answering", data.error,
            ui.copy("cargo run -p kryta-supervisor"))));
          return;
        }
        holder.appendChild(ui.card(["Running stack", h("span", { class: "spacer" }), stamp], ui.table({
          columns: [
            { label: "Service", value: function (row) { return h("b", { text: row.name }); } },
            { label: "State", value: function (row) { return ui.badge(row.state, row.up ? "ok" : "warn", row.up ? "live" : true); } },
            { label: "Port", num: true, value: function (row) { return row.port || "-"; } },
            { label: "", class: "actions", value: function (row) {
              return [ui.button("Restart", function () { act("restart", row.name); }, "small"),
                ui.button("Stop", function () { act("stop", row.name); }, "small danger")];
            } }
          ],
          rows: data.services,
          empty: { title: "The supervisor runs no service", text: "Its stack.json lists none." }
        }), { flush: true }));
      }

      view.head({ actions: ui.button("Stop the stack", async function () {
        var sure = await ui.confirm({
          title: "Stop every service?",
          text: "The supervisor stops every service it runs. When this panel is one of them, it stops too and this page goes quiet.",
          label: "Stop the stack", danger: true
        });
        if (!sure) return;
        await ui.act(function () { return api.post("/services/stop-all"); }, function (data) { return "Supervisor: " + data.answer; });
        refresh();
      }, "danger") });
      view.body(holder, h("p", { class: "muted small" }, h("br"),
        "The list refreshes every four seconds. The panel asks; the supervisor remains the only owner of its processes."));
      await refresh();
      view.every(4000, refresh);
    }
  });
})();
