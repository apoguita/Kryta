/* Console: the operations the running world registers, with a form built from its own list. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  Kryta.area({
    id: "console",
    label: "Console",
    group: "Server",
    hint: "operations the running world registers",
    render: async function (view) {
      var data = await api.get("/console/operations");
      var result = h("div", null, ui.empty("No operation run yet", "Pick an operation and run it; the world's answer appears here."));
      var formHolder = h("div");
      var operations = data.operations;

      function show(title, text, bad) {
        Kryta.clear(result);
        result.appendChild(ui.check(ui.badge(bad ? "refused" : "answered", bad ? "bad" : "ok", true), title));
        result.appendChild(ui.log(text || "(an empty result)"));
      }

      function drawForm(name) {
        var operation = operations.filter(function (one) { return one.name === name; })[0];
        var known = operation && operation.arguments;
        var fields = [{
          name: "operation", label: "Operation", full: true,
          type: operations.length ? "select" : "text", value: name, placeholder: "world.status",
          options: operations.map(function (one) { return { value: one.name, label: one.name }; }),
          hint: operation ? operation.help : "The world's list is unavailable, so type the name.",
          onChange: function (value) { drawForm(value); }
        }];
        if (known) {
          known.forEach(function (argument) { fields.push({ name: "arg:" + argument, label: argument }); });
        } else {
          fields.push({ name: "arguments_text", label: "Arguments", type: "textarea", full: true,
            placeholder: "destination=ascalon", hint: "One name=value per line. The world decides whether they are valid." });
        }
        Kryta.clear(formHolder).appendChild(ui.form({
          fields: fields, submit: "Run",
          onSubmit: async function (values) {
            var body = { operation: values.operation, arguments: {}, arguments_text: values.arguments_text || "" };
            Object.keys(values).forEach(function (key) {
              if (key.indexOf("arg:") === 0) body.arguments[key.slice(4)] = values[key];
            });
            try {
              var answer = await api.post("/console/run", body);
              show(answer.operation, answer.result, false);
              ui.toast(answer.operation + " answered.");
            } catch (error) {
              show(values.operation, error.message, true);
              throw error;
            }
          }
        }));
        if (known && !known.length) formHolder.appendChild(h("p", { class: "muted small", text: "This operation takes no values." }));
      }

      drawForm(operations.length ? (operations.filter(function (one) { return one.name === "world.status"; })[0] || operations[0]).name : "");
      view.body(
        data.available ? null : ui.banner("The running world did not answer, so its list of operations is not available: " + data.error),
        ui.grid("g2",
          ui.card("Run an operation", formHolder),
          ui.card("Result", result)),
        h("br"),
        ui.card(["Registered operations", h("span", { class: "muted", text: operations.length + " from world.operations" })], ui.table({
          columns: [
            { label: "Operation", sort: function (row) { return row.name; }, value: function (row) { return h("code", { text: row.name }); } },
            { label: "What it does", value: function (row) { return row.help; } },
            { label: "Values", value: function (row) {
              return row.arguments === null ? "see the help text" : row.arguments.length ? row.arguments.join(", ") : h("span", { class: "muted", text: "none" });
            } }
          ],
          rows: operations,
          onRow: function (row) { drawForm(row.name); window.scrollTo(0, 0); },
          empty: { title: "The world is not answering", text: "Start it from Services, or check ControlPort and ControlCredentialFile in manage.json." }
        }), { flush: true }));
    }
  });
})();
