/* Recordings: build the capture command to run by hand, and manage the library. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api, fmt = Kryta.fmt;

  function two(value) { return (value < 10 ? "0" : "") + value; }

  /* YYYYMMDD-HHMMSS, the stamp the naming rule gives a tool that names its own output. */
  function stamp(date) {
    return "" + date.getFullYear() + two(date.getMonth() + 1) + two(date.getDate()) + "-" +
      two(date.getHours()) + two(date.getMinutes()) + two(date.getSeconds());
  }

  /* A subject is a folder and a file name: lower-case letters, digits, dash and underscore. */
  function cleanSubject(text) {
    return text.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  }

  /* The exact PowerShell lines for one capture. */
  function command(preflight, choice) {
    var subject = cleanSubject(choice.subject) || "subject";
    var out = "recordings/client/feature/" + subject + "/" + subject + "-" + stamp(new Date()) + ".jsonl";
    var python = preflight.python.found ? "& \"" + preflight.python.path + "\"" : "python";
    var options = [];
    if (choice.pid) options.push("--pid " + choice.pid);
    options.push("--dir " + choice.direction, "--seconds " + choice.seconds);
    if (!choice.mask) options.push("--raw");
    // One short line each, joined with PowerShell's backtick, so the whole command is readable.
    var lines = [python, "  " + preflight.script, "  " + options.join(" "), "  --out " + out];
    return { out: out, text: "cd \"" + (preflight.root || ".") + "\"\n" + lines.join(" `\n") };
  }

  function found(ok, yes, no) {
    return ui.badge(ok ? yes : no, ok ? "ok" : "bad", true);
  }

  function preflightCard(preflight, reload) {
    var clients = preflight.clients;
    return ui.card(["Before you start", h("span", { class: "spacer" }), ui.button("Check again", reload, "small")], [
      ui.check(found(preflight.python.found, "found", "missing"), "32-bit Python",
        preflight.python.found ? preflight.python.path : "Looked in: " + (preflight.python.looked.join(", ") || "no place is known; set KRYTA_PY32")),
      ui.check(found(preflight.stealth.found, "found", "missing"), "Py4GW Stealth",
        (preflight.stealth.path || "set PY4GW_STEALTH") + (preflight.stealth.found ? "" : " - no py4gw folder there")),
      ui.check(preflight.clients_error ? ui.badge("unknown", "warn", true) : found(clients.length > 0, clients.length + " running", "none running"), "Game client",
        preflight.clients_error || (clients.length ? clients.map(function (one) { return one.name + " (pid " + one.pid + ")"; }).join(", ") : "Start the client, then check again.")),
      ui.check(found(preflight.script_found, "found", "missing"), "Capture script", preflight.script),
      ui.check(ui.badge("by hand", "warn", true), "Elevated shell", "The panel cannot open one for you, and cannot tell whether yours is.")
    ]);
  }

  async function recordClient(view) {
    var preflight = await api.get("/recordings/preflight");
    var choice = { pid: preflight.clients.length ? String(preflight.clients[0].pid) : "", direction: "both", seconds: "0", subject: "", mask: true };
    var block = ui.copy("");
    var outLine = h("code");
    var subjectNote = ui.banner("Type a subject: it names the folder and the file. Until then the command says \"subject\".");
    var rawWarning = ui.banner("Masking is off: the file will hold player names, chat text and account ids in clear. Keep it on this machine and never commit it.", "bad");
    function redraw() {
      var built = command(preflight, choice);
      block.update(built.text);
      outLine.textContent = built.out;
      subjectNote.hidden = cleanSubject(choice.subject) !== "";
      rawWarning.hidden = choice.mask;
    }
    var set = function (key) { return function (value) { choice[key] = value; redraw(); }; };
    var subject = h("input", { type: "text", placeholder: "title-select", value: "", spellcheck: "false", autocomplete: "off",
      on: { input: function (event) { choice.subject = event.target.value; redraw(); } } });
    var mask = h("input", { type: "checkbox", checked: true, on: { change: function (event) { choice.mask = event.target.checked; redraw(); } } });
    var clientOptions = preflight.clients.map(function (one) { return { value: one.pid, label: one.name + " - pid " + one.pid }; });
    clientOptions.push({ value: "", label: preflight.clients.length ? "let the script find it" : "no client is running" });
    redraw();

    return ui.grid("g3",
      ui.card("Record a client", [
        h("p", { class: "muted", text: "A client capture needs an elevated shell, which this panel does not have. Fill in the capture and run the command yourself." }),
        h("br"),
        h("div", { class: "form wide" },
          h("label", { class: "field" }, "Client", ui.select(clientOptions, set("pid"), choice.pid)),
          h("label", { class: "field" }, "Direction", ui.select([
            { value: "both", label: "both" }, { value: "StoC", label: "server to client" }, { value: "CToS", label: "client to server" }], set("direction"), choice.direction)),
          h("label", { class: "field" }, "Duration", ui.select([
            { value: "0", label: "until I stop it" }, { value: "30", label: "30 seconds" }, { value: "90", label: "90 seconds" },
            { value: "300", label: "5 minutes" }, { value: "900", label: "15 minutes" }], set("seconds"), choice.seconds)),
          h("label", { class: "field" }, "Subject", subject, h("span", { class: "hint", text: "what is being recorded, in a word or two" }))),
        h("br"),
        h("label", { class: "checkline" }, mask, "Mask names and text (leave on unless the capture stays on this machine)"),
        h("br"), h("br"), subjectNote, rawWarning,
        ui.steps([
          { title: "Open an elevated PowerShell", text: "Start menu, type PowerShell, right-click, \"Run as administrator\". Approve the prompt." },
          { title: "Paste this command", text: "It goes to the repository, then starts the capture on the client you picked.",
            extra: [block, h("p", { class: "small" }, "Writes ", outLine)] },
          { title: "Do the thing you want recorded", text: "The window prints \"--- capturing ...\" when the hooks are in. If it stays quiet for a minute it prints \"nothing for 60s - still capturing\", which means the client is idle." },
          { title: "Stop it with Ctrl+C in that window", text: "Never close the window: Ctrl+C lets the script take its hooks out of the client and write its summary record. A timed capture stops by itself." },
          { title: "Come back to the library", text: "The new file is listed there; describe it and add it to the index.",
            extra: ui.button("Open the library", function () { Kryta.go("#/recordings/library"); }, "small") }
        ])]),
      preflightCard(preflight, view.reload));
  }

  function recordServer() {
    return ui.card("Record the server", [
      h("p", { class: "muted", text: "The world can write every game message it sends and receives, in both directions, to a file. It needs no elevation - only a setting and a restart. The panel does not edit settings, so this is done by hand." }),
      h("br"),
      ui.steps([
        { title: "Name the file in worldserver.json", text: "Set CaptureFile to a path. The file holds payloads unsanitised, so use a scratch path or a path under recordings/ you will not commit.",
          extra: ui.copy("\"CaptureFile\": {\n  \"value\": \"logs/world-capture.jsonl\"\n}") },
        { title: "Restart the world", text: "The setting is read once, at start-up.",
          extra: ui.button("Open services", function () { Kryta.go("#/services"); }, "small") },
        { title: "Play the thing you want recorded", text: "Every connection is written to the one file. Starting the world again overwrites it, so copy it away before the next restart." },
        { title: "Switch it off", text: "Set CaptureFile back to an empty value and restart the world again." }
      ])]);
  }

  function describe(row, refresh) {
    var drawer = ui.drawer(row.name, ui.skeleton(4));
    api.get("/recordings/describe?path=" + encodeURIComponent(row.path)).then(function (data) {
      var header = data.header || {};
      var summary = data.summary;
      Kryta.clear(drawer.body);
      [
        ui.kv([
          ["Path", h("code", { text: "recordings/" + data.path })],
          ["Source / kind", data.scheme.source + " / " + data.scheme.kind + (data.scheme.feature ? " / " + data.scheme.feature : "")],
          ["Subject", data.scheme.subject],
          ["Size", fmt.bytes(data.bytes)],
          ["Messages", fmt.number(data.messages)],
          ["Indexed", data.indexed ? "yes" : "no"]
        ]),
        h("h3", { text: "Header record" }),
        data.header ? ui.kv([
          ["Tool", header.tool],
          ["Started", header.started_at],
          ["Layer", header.layer ? header.layer + (header.header_len ? ", " + header.header_len + "-byte headers" : "") : "not stated (an older capture)"],
          ["Directions", header.directions],
          ["Sanitised", header.sanitised === undefined ? "not stated" : header.sanitised ? "yes" : "NO - holds names and ids"],
          ["Client", header.client ? (header.client.name || "client") + " pid " + header.client.pid + ", version " + header.client.file_version : null],
          ["Notes", header.notes]
        ].filter(function (pair) { return pair[1] !== undefined && pair[1] !== null; }))
          : h("p", { class: "muted", text: "The first line is not a capture header, so this file says nothing about itself." }),
        h("h3", { text: "Summary record" }),
        summary ? [
          ui.kv([["Seconds", String(summary.seconds)], ["Packets", fmt.number(summary.packets)],
            summary.raw ? ["Raw", "yes"] : null]),
          h("br"),
          ui.table({
            columns: [
              { label: "Message", value: function (item) { return h("code", { text: item.seen }); } },
              { label: "Count", num: true, value: function (item) { return fmt.number(item.count); } }
            ], rows: summary.headers || [], limit: 60
          })
        ] : h("p", { class: "muted", text: "The last line is not a summary record: an older capture, or one that was killed instead of stopped." }),
        data.indexed ? null : h("div", { class: "form-foot" }, ui.button("Add to index", function () { drawer.close(); addToIndex(row, refresh); }, "primary"))
      ].forEach(function (part) { if (part) drawer.body.appendChild(part.nodeType ? part : h("div", null, part)); });
    }).catch(function (error) {
      Kryta.clear(drawer.body).appendChild(ui.empty("This recording could not be described", error.message));
    });
  }

  function addToIndex(row, refresh) {
    ui.formModal({
      title: "Add to the index",
      intro: "Appends one line to recordings/index.jsonl for " + row.path + ": its source, kind, subject, path, when it was recorded and its tool, read from the folder and the file's header.",
      submit: "Add to index",
      fields: [{ name: "note", label: "Note (optional)", type: "textarea", full: true, placeholder: "what the recording shows, and against which server" }],
      onSubmit: async function (values) {
        await api.post("/recordings/index", { path: row.path, note: values.note });
        ui.toast(row.name + " is now in the index.");
        refresh();
      }
    });
  }

  async function library(view) {
    var data = await api.get("/recordings");
    var typed = "", show = "all";
    var refresh = function () { Kryta.recount("recordings"); view.reload(); };
    var table = ui.table({
      columns: [
        { label: "Recording", sort: function (row) { return row.name.toLowerCase(); },
          value: function (row) { return [h("b", { text: row.name }), h("div", { class: "sub", text: row.path.slice(0, row.path.length - row.name.length - 1) })]; } },
        { label: "Source", sort: function (row) { return row.source; }, value: function (row) { return row.source; } },
        { label: "Kind", sort: function (row) { return row.kind; }, value: function (row) { return row.kind + (row.feature ? " / " + row.feature : ""); } },
        { label: "Layer", value: function (row) { return row.layer || h("span", { class: "muted", text: "-" }); } },
        { label: "Size", num: true, sort: function (row) { return row.bytes; }, value: function (row) { return fmt.bytes(row.bytes); } },
        { label: "Sanitised", sort: function (row) { return row.sanitised === false ? 0 : 1; }, value: function (row) {
          return row.sanitised === false ? ui.badge("unsanitised", "bad", true) : row.sanitised === true ? ui.badge("yes", "ok") : ui.badge("n/a", "plain");
        } },
        { label: "Indexed", sort: function (row) { return row.indexed ? 1 : 0; }, value: function (row) {
          return row.indexed ? ui.badge("yes", "ok") : ui.badge("not indexed", "warn", true);
        } },
        { label: "", class: "actions", value: function (row) {
          return [ui.button("Describe", function () { describe(row, refresh); }, "small"),
            row.indexed ? null : ui.button("Add to index", function () { addToIndex(row, refresh); }, "small")];
        } }
      ],
      onRow: function (row) { describe(row, refresh); },
      empty: data.recordings.length
        ? { title: "No recording matches", text: "Change the search or the filter." }
        : { title: "No recording yet", text: "Make one from the \"Record a client\" tab; it appears here when the capture stops." }
    });
    var summary = h("span", { class: "muted small" });
    function filter() {
      var rows = data.recordings.filter(function (row) {
        return (show === "all" || (show === "raw" && row.sanitised === false) || (show === "unindexed" && !row.indexed)) &&
          (!typed || ui.matches(typed, [row.path, row.tool, row.subject]));
      });
      summary.textContent = fmt.plural(rows.length, "recording") + " of " + data.recordings.length + ", " + fmt.bytes(data.bytes) + " in all";
      table.update(rows);
    }
    filter();
    return [
      ui.toolbar(
        ui.search("Search by name, folder or tool", function (value) { typed = value; filter(); }),
        ui.select([{ value: "all", label: "Everything" }, { value: "raw", label: "Unsanitised" }, { value: "unindexed", label: "Not indexed" }],
          function (value) { show = value; filter(); }, show, "Show"),
        summary),
      ui.card(null, table, { flush: true })
    ];
  }

  Kryta.area({
    id: "recordings",
    label: "Recordings",
    group: "Research",
    hint: "make one, find one, index it",
    count: async function () { return (await api.get("/recordings")).recordings.length; },
    render: async function (view, route) {
      view.body(ui.tabs([
        { id: "client", label: "Record a client", render: function () { return recordClient(view); } },
        { id: "server", label: "Record the server", render: recordServer },
        { id: "library", label: "Library", render: function () { return library(view); } }
      ], route.parts[0] || "client", function (id) {
        window.history.replaceState(null, "", "#/recordings/" + id);
      }));
    }
  });
})();
