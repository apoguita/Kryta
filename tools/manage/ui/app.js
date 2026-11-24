/* Kryta Control - the shell: the area list, the router, the service client and the building
   blocks every area page is made from. Plain JavaScript, no build step, no library.

   Untrusted text never becomes markup here: every node is built with h(), which sets textContent.
   There are no inline handlers and nothing is evaluated. */
(function () {
  "use strict";

  // THE AREA LIST. Each file under ui/areas/ adds itself with Kryta.area({...}); the menu, the
  // router and the Ctrl+K palette are all built from this one list, in the order the service
  // loads the files (its area table, tools/manage/src/areas/mod.rs). Nothing else lists areas.
  // How to add one: docs/port/23-control-panel.md, "Adding an area".
  var areas = [];
  var Kryta = (window.Kryta = {});

  /* Register an area:
       id      its name: the "#/<id>" page and the "/api/v1/<id>" prefix
       label   what the menu calls it
       group   the menu heading it sits under (a new name makes a new heading)
       hint    one line under the page title
       render  async function (view, route): draw the page into the view
       count   optional async function: the number shown beside the menu entry
       find    optional async function: [{label, hint, href}] for the Ctrl+K palette */
  Kryta.area = function (definition) {
    areas.push(definition);
  };

  // ---- nodes -----------------------------------------------------------------------------------

  /* h("div", {class: "card", on: {click: fn}}, child, "text", [more]) */
  function h(tag, attributes) {
    var node = document.createElement(tag);
    var name;
    for (name in attributes || {}) {
      var value = attributes[name];
      if (value === null || value === undefined || value === false) continue;
      if (name === "class") node.className = value;
      else if (name === "text") node.textContent = value;
      else if (name === "on") for (var type in value) node.addEventListener(type, value[type]);
      else if (name === "value") node.value = value;
      else if (name === "checked" || name === "disabled" || name === "selected") node[name] = true;
      else node.setAttribute(name, value === true ? "" : String(value));
    }
    for (var index = 2; index < arguments.length; index++) append(node, arguments[index]);
    return node;
  }

  function append(node, child) {
    if (child === null || child === undefined || child === false) return;
    if (Array.isArray(child)) child.forEach(function (one) { append(node, one); });
    else node.appendChild(child.nodeType ? child : document.createTextNode(String(child)));
  }

  function clear(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
    return node;
  }

  /* The one escape helper, for the rare place a string is needed instead of a node. */
  function esc(text) {
    return String(text).replace(/[&<>"']/g, function (character) {
      return "&#" + character.charCodeAt(0) + ";";
    });
  }

  // ---- the service -----------------------------------------------------------------------------

  var TOKEN_KEY = "kryta.token";

  function stored(storage, key) {
    try { return storage.getItem(key); } catch (error) { return null; }
  }

  function store(storage, key, value) {
    try {
      if (value === null) storage.removeItem(key); else storage.setItem(key, value);
    } catch (error) { /* a browser that refuses storage still works for this page load */ }
  }

  async function request(method, path, body) {
    // Every request says it comes from the panel's own pages; the service refuses a change
    // without this header, which another site's form cannot send.
    var headers = { "Accept": "application/json", "X-Kryta-Panel": "1" };
    var token = stored(window.sessionStorage, TOKEN_KEY);
    if (token) headers.Authorization = "Bearer " + token;
    var options = { method: method, headers: headers, cache: "no-store" };
    if (method === "POST") {
      headers["Content-Type"] = "application/json";
      options.body = JSON.stringify(body || {});
    }
    var response, data;
    try {
      response = await fetch("/api/v1" + path, options);
    } catch (error) {
      throw new Error("The panel service did not answer. Is it still running?");
    }
    try {
      data = await response.json();
    } catch (error) {
      data = { ok: false, error: "The service answered something that is not JSON (status " + response.status + ")." };
    }
    if (response.status === 401) unlock();
    if (!data.ok) {
      var failure = new Error(data.error || "The request failed (status " + response.status + ").");
      failure.status = response.status;
      throw failure;
    }
    return data;
  }

  var api = {
    get: function (path) { return request("GET", path); },
    post: function (path, body) { return request("POST", path, body); }
  };

  // ---- small pieces ----------------------------------------------------------------------------

  var ui = {};

  /* kind: "ok" | "warn" | "bad" | "plain"; dot adds the status dot, live makes it pulse. */
  ui.badge = function (text, kind, dot) {
    return h("span", { class: "pill " + (kind || "plain") + (dot ? " dot" : "") + (dot === "live" ? " live" : ""), text: text });
  };

  /* kind: "", "primary", "danger", "small", or several. */
  ui.button = function (label, onClick, kind) {
    return h("button", { class: "btn " + (kind || ""), type: "button", on: { click: onClick } }, label);
  };

  ui.link = function (label, href, kind) {
    return h("a", { class: kind === undefined ? null : "btn " + kind, href: href }, label);
  };

  /* A card: title may be a string, a node or null; options.flush drops the body padding (tables). */
  ui.card = function (title, body, options) {
    var card = h("div", { class: "card" + ((options && options.class) ? " " + options.class : "") });
    if (title) card.appendChild(h("h2", null, title));
    if (options && options.flush) append(card, body);
    else card.appendChild(h("div", { class: "body" }, body));
    return card;
  };

  ui.stat = function (label, value, note) {
    return h("div", { class: "card stat" }, h("div", { class: "body" },
      h("div", { class: "label", text: label }),
      h("div", { class: "value", text: value === null || value === undefined ? "-" : String(value) }),
      note ? h("div", { class: "note", text: note }) : null));
  };

  ui.grid = function (kind) {
    return h("div", { class: "grid " + kind }, Array.prototype.slice.call(arguments, 1));
  };

  ui.toolbar = function () {
    return h("div", { class: "toolbar" }, Array.prototype.slice.call(arguments));
  };

  ui.search = function (placeholder, onInput, value) {
    return h("input", {
      class: "search", type: "search", placeholder: placeholder, value: value || "", "aria-label": placeholder,
      on: { input: function (event) { onInput(event.target.value); } }
    });
  };

  /* options: [{value, label}] or plain strings. */
  ui.select = function (options, onChange, value, label) {
    var select = h("select", { "aria-label": label || null, on: { change: function (event) { onChange(event.target.value); } } },
      options.map(function (option) {
        var item = typeof option === "object" ? option : { value: option, label: option };
        return h("option", { value: item.value, text: item.label });
      }));
    if (value !== undefined && value !== null) select.value = String(value);
    return select;
  };

  ui.empty = function (title, text, action) {
    return h("div", { class: "empty" }, h("div", { class: "glyph" }), h("h3", { text: title }),
      text ? h("p", { text: text }) : null, action || null);
  };

  ui.skeleton = function (lines) {
    var node = h("div", { class: "skeleton", "aria-busy": "true" },
      h("div", { class: "row" }, h("i", { class: "block" }), h("i", { class: "block" }), h("i", { class: "block" }), h("i", { class: "block" })));
    for (var index = 0; index < (lines || 5); index++) node.appendChild(h("i"));
    return node;
  };

  ui.banner = function (text, kind) {
    return h("div", { class: "banner " + (kind || ""), text: text });
  };

  ui.kv = function (pairs) {
    return h("dl", { class: "kv" }, pairs.filter(Boolean).map(function (pair) {
      return [h("dt", { text: pair[0] }), h("dd", null, pair[1] === null || pair[1] === undefined ? "-" : pair[1])];
    }));
  };

  ui.check = function (badge, title, detail, href) {
    return h(href ? "a" : "div", { class: "check", href: href || null }, badge,
      h("div", { class: "what" }, title, detail ? h("small", { text: detail }) : null));
  };

  ui.steps = function (steps) {
    return h("ol", { class: "steps" }, steps.map(function (step) {
      return h("li", null, h("div", null, h("h3", { text: step.title }), step.text ? h("p", { text: step.text }) : null, step.extra || null));
    }));
  };

  /* An item's name as the rows store it. Most are the client's encoded string references, not
     text, so anything that is not plain printable text is shown as its code units. */
  ui.itemName = function (name) {
    if (!name) return h("span", { class: "muted", text: "(unnamed)" });
    if (/^[ -~]+$/.test(name)) return name;
    var units = [];
    for (var index = 0; index < name.length; index++) units.push(("000" + name.charCodeAt(index).toString(16)).slice(-4));
    return h("code", { class: "muted", title: "an encoded client string reference, not text", text: units.join(" ") });
  };

  ui.log = function (text) {
    return h("div", { class: "log", text: text });
  };

  /* A block of text to paste somewhere else, with a Copy button. update(text) replaces it. */
  ui.copy = function (text) {
    var pre = h("pre", { text: text });
    var button = ui.button("Copy", async function () {
      try {
        await navigator.clipboard.writeText(pre.textContent);
        ui.toast("Copied to the clipboard.");
      } catch (error) {
        var range = document.createRange();
        range.selectNodeContents(pre);
        var selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        ui.toast("The browser refused the clipboard; the text is selected, press Ctrl+C.", "warn");
      }
    }, "small");
    var node = h("div", { class: "cmd" }, pre, button);
    node.update = function (next) { pre.textContent = next; };
    return node;
  };

  var fmt = {
    number: function (value) {
      return value === null || value === undefined ? "-" : Number(value).toLocaleString("en-US");
    },
    bytes: function (bytes) {
      if (bytes < 1024) return bytes + " B";
      if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(bytes < 10240 ? 1 : 0) + " kB";
      return (bytes / 1024 / 1024).toFixed(1) + " MB";
    },
    plural: function (count, word) {
      return fmt.number(count) + " " + word + (count === 1 ? "" : "s");
    }
  };

  // ---- table -----------------------------------------------------------------------------------

  /* ui.table({
       columns: [{label, value: row => text or node, num: true, sort: row => comparable, class}],
       rows, empty: {title, text, action}, onRow: row => ..., limit: 300, note: "text under it"
     })
     Returns a node with update(rows). A column with sort can be clicked to order by it. */
  ui.table = function (options) {
    var limit = options.limit || 300;
    var order = { column: null, descending: false };
    var rows = options.rows || [];
    var wrap = h("div", { class: "table-wrap" });
    var holder = h("div", null, wrap);

    function draw() {
      clear(wrap);
      while (holder.childNodes.length > 1) holder.removeChild(holder.lastChild);
      if (!rows.length) {
        var empty = options.empty || { title: "Nothing here yet" };
        wrap.appendChild(ui.empty(empty.title, empty.text, empty.action));
        return;
      }
      var shown = rows.slice();
      if (order.column) {
        var key = order.column.sort;
        shown.sort(function (left, right) {
          var a = key(left), b = key(right);
          return (a < b ? -1 : a > b ? 1 : 0) * (order.descending ? -1 : 1);
        });
      }
      var head = h("tr", null, options.columns.map(function (column) {
        var state = order.column === column ? (order.descending ? " desc" : " asc") : "";
        return h("th", {
          class: (column.num ? "num " : "") + (column.sort ? "sortable" : "") + state,
          on: column.sort ? { click: function () {
            order.descending = order.column === column ? !order.descending : false;
            order.column = column;
            draw();
          } } : null
        }, column.label, column.sort ? h("span", { class: "arrow" }) : null);
      }));
      var body = h("tbody", null, shown.slice(0, limit).map(function (row) {
        return h("tr", {
          class: options.onRow ? "link" : null,
          tabindex: options.onRow ? "0" : null,
          on: options.onRow ? {
            click: function (event) { if (!event.target.closest("button, a, input")) options.onRow(row); },
            keydown: function (event) { if (event.key === "Enter" && event.target === event.currentTarget) options.onRow(row); }
          } : null
        }, options.columns.map(function (column) {
          return h("td", { class: (column.num ? "num " : "") + (column.class || "") }, column.value(row));
        }));
      }));
      wrap.appendChild(h("table", null, h("thead", null, head), body));
      var notes = [];
      if (shown.length > limit) notes.push("Showing the first " + limit + " of " + fmt.number(shown.length) + ". Narrow the search to see the rest.");
      if (options.note) notes.push(options.note);
      if (notes.length) holder.appendChild(h("div", { class: "table-note", text: notes.join(" ") }));
    }

    holder.update = function (next) { rows = next || []; draw(); };
    draw();
    return holder;
  };

  /* Whether any of the texts contains every word typed. */
  ui.matches = function (typed, texts) {
    var hay = texts.filter(Boolean).join(" ").toLowerCase();
    return typed.toLowerCase().split(/\s+/).every(function (word) { return hay.indexOf(word) !== -1; });
  };

  // ---- tabs ------------------------------------------------------------------------------------

  /* ui.tabs([{id, label, render: panel => node | Promise<node>}], activeId, onChange) */
  ui.tabs = function (tabs, active, onChange) {
    var bar = h("div", { class: "tabs", role: "tablist" });
    var panel = h("div", { class: "tab-panel" });
    var buttons = {};

    async function show(tab) {
      for (var id in buttons) buttons[id].classList.toggle("on", id === tab.id);
      clear(panel).appendChild(ui.skeleton(4));
      panel.classList.remove("tab-panel");
      void panel.offsetWidth;
      panel.classList.add("tab-panel");
      try {
        var content = await tab.render(panel);
        if (buttons[tab.id].classList.contains("on") && content) append(clear(panel), content);
      } catch (error) {
        clear(panel).appendChild(ui.empty("This could not be loaded", error.message));
      }
    }

    tabs.forEach(function (tab) {
      buttons[tab.id] = h("button", { type: "button", role: "tab", on: { click: function () {
        if (onChange) onChange(tab.id);
        show(tab);
      } } }, tab.label);
      bar.appendChild(buttons[tab.id]);
    });
    show(tabs.filter(function (tab) { return tab.id === active; })[0] || tabs[0]);
    return h("div", null, bar, panel);
  };

  // ---- toasts ----------------------------------------------------------------------------------

  ui.toast = function (text, kind) {
    var toast = h("div", { class: "toast " + (kind || ""), role: "status", text: text });
    document.getElementById("toasts").appendChild(toast);
    setTimeout(function () {
      toast.classList.add("out");
      setTimeout(function () { toast.remove(); }, 250);
    }, kind === "bad" ? 7000 : 3500);
  };

  /* Run an action and say how it went. Returns true when it succeeded. */
  ui.act = async function (action, done) {
    try {
      var result = await action();
      if (done) ui.toast(typeof done === "function" ? done(result) : done);
      return true;
    } catch (error) {
      ui.toast(error.message, "bad");
      return false;
    }
  };

  // ---- overlays --------------------------------------------------------------------------------

  var overlays = [];

  function overlay(kind, content, onClose) {
    var previous = document.activeElement;
    var scrim = h("div", { class: "scrim " + kind, on: { mousedown: function (event) {
      if (event.target === scrim) close();
    } } }, content);
    function close() {
      var index = overlays.indexOf(close);
      if (index === -1) return;
      overlays.splice(index, 1);
      scrim.remove();
      if (previous && previous.focus) previous.focus();
      if (onClose) onClose();
    }
    overlays.push(close);
    document.body.appendChild(scrim);
    var first = content.querySelector("input, select, textarea, button.primary") || content.querySelector("button");
    if (first) first.focus();
    return close;
  }

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape" && overlays.length) {
      event.preventDefault();
      overlays[overlays.length - 1]();
    }
  });

  /* ui.modal({title, body, actions: [nodes], wide}) -> {close} */
  ui.modal = function (options) {
    var close;
    var box = h("div", { class: "modal" + (options.wide ? " wide" : ""), role: "dialog", "aria-modal": "true", "aria-label": options.title },
      h("header", null, h("h2", { text: options.title }), ui.button("Close", function () { close(); }, "small")),
      h("div", { class: "body" }, options.body),
      options.actions ? h("footer", null, options.actions) : null);
    close = overlay("", box, options.onClose);
    return { close: close };
  };

  /* ui.confirm({title, text, label, danger}) -> Promise<boolean>; never the browser's confirm(). */
  ui.confirm = function (options) {
    return new Promise(function (resolve) {
      var answered = false;
      var modal = ui.modal({
        title: options.title,
        body: h("p", { text: options.text }),
        onClose: function () { if (!answered) resolve(false); },
        actions: [
          ui.button("Cancel", function () { modal.close(); }),
          ui.button(options.label || "Confirm", function () { answered = true; modal.close(); resolve(true); },
            options.danger ? "danger" : "primary")
        ]
      });
    });
  };

  /* ui.drawer(title, content) -> {close, body}: details slide in from the right. */
  ui.drawer = function (title, content) {
    var close;
    var body = h("div", { class: "body" }, content);
    var box = h("aside", { class: "drawer", role: "dialog", "aria-label": title },
      h("header", null, h("h2", { text: title }), ui.button("Close", function () { close(); }, "small")), body);
    close = overlay("side", box);
    return { close: close, body: body };
  };

  // ---- forms -----------------------------------------------------------------------------------

  /* ui.form({
       fields: [{name, label, type: "text"|"number"|"password"|"select"|"textarea"|"checkbox",
                 value, options, placeholder, hint, min, max, full}],
       submit: "Save", kind: "primary" | "danger", wide, cancel: fn,
       onSubmit: async values => ...   (throw to refuse; the form keeps what was typed)
     })
     An error is shown beside the button and as a toast; an error whose message names a field's
     label marks that field. */
  ui.form = function (options) {
    var inputs = {};
    var labels = {};
    var error = h("span", { class: "form-error", role: "alert" });

    var fields = options.fields.map(function (field) {
      var input;
      if (field.type === "select") {
        input = ui.select(field.options, function () {}, field.value);
      } else if (field.type === "textarea") {
        input = h("textarea", { placeholder: field.placeholder || null, value: field.value || "" });
      } else if (field.type === "checkbox") {
        input = h("input", { type: "checkbox", checked: !!field.value });
      } else {
        input = h("input", {
          type: field.type || "text", value: field.value === undefined || field.value === null ? "" : field.value,
          placeholder: field.placeholder || null, min: field.min, max: field.max,
          autocomplete: "off", spellcheck: "false"
        });
      }
      input.name = field.name;
      inputs[field.name] = input;
      if (field.onChange) input.addEventListener("change", function () { field.onChange(input.value, inputs); });
      var label = field.type === "checkbox"
        ? h("label", { class: "checkline" + (field.full ? " full" : "") }, input, field.label)
        : h("label", { class: "field" + (field.full ? " full" : "") }, field.label, input,
            field.hint ? h("span", { class: "hint", text: field.hint }) : null);
      labels[field.name] = label;
      return label;
    });

    var submit = h("button", { class: "btn " + (options.kind || "primary"), type: "submit" }, options.submit || "Save");
    var form = h("form", { novalidate: true, on: { submit: async function (event) {
      event.preventDefault();
      var values = {};
      options.fields.forEach(function (field) {
        var input = inputs[field.name];
        values[field.name] = field.type === "checkbox" ? input.checked : input.value.trim();
        labels[field.name].classList.remove("invalid");
      });
      error.textContent = "";
      submit.disabled = true;
      submit.classList.add("busy");
      try {
        await options.onSubmit(values);
      } catch (failure) {
        error.textContent = failure.message;
        options.fields.forEach(function (field) {
          var word = String(field.label).split(" ")[0].toLowerCase();
          if (failure.message.toLowerCase().indexOf(word) !== -1) labels[field.name].classList.add("invalid");
        });
        ui.toast(failure.message, "bad");
      }
      submit.disabled = false;
      submit.classList.remove("busy");
    } } },
      h("div", { class: "form" + (options.wide ? " wide" : "") }, fields),
      h("div", { class: "form-foot" }, submit,
        options.cancel ? ui.button("Cancel", options.cancel) : null, error));
    form.inputs = inputs;
    return form;
  };

  /* A form in a modal: closes itself when onSubmit succeeds. */
  ui.formModal = function (options) {
    var modal;
    var form = ui.form({
      fields: options.fields, submit: options.submit, kind: options.kind, wide: options.wide,
      cancel: function () { modal.close(); },
      onSubmit: async function (values) {
        await options.onSubmit(values);
        modal.close();
      }
    });
    modal = ui.modal({ title: options.title, wide: options.wide, body: [options.intro ? h("p", { class: "muted", text: options.intro }) : null, options.intro ? h("br") : null, form] });
    return modal;
  };

  // ---- the token -------------------------------------------------------------------------------

  var unlocking = false;

  function unlock() {
    if (unlocking) return;
    unlocking = true;
    var form = ui.form({
      fields: [{ name: "token", label: "Token", type: "password", full: true, hint: "The Token setting in manage.json. It is kept for this browser tab only." }],
      submit: "Unlock",
      onSubmit: async function (values) {
        store(window.sessionStorage, TOKEN_KEY, values.token);
        var probe = await fetch("/api/v1/overview", { headers: { "Authorization": "Bearer " + values.token, "X-Kryta-Panel": "1" }, cache: "no-store" });
        if (probe.status === 401) {
          store(window.sessionStorage, TOKEN_KEY, null);
          throw new Error("That is not the token.");
        }
        screen.remove();
        unlocking = false;
        buildRail();
        navigate();
      }
    });
    var screen = h("div", { class: "unlock" }, h("div", { class: "modal" }, h("div", { class: "body" },
      h("div", { class: "brand" }, h("div", { class: "mark", text: "K" }), h("span", { text: "Kryta Control" })),
      h("p", { class: "muted", text: "This panel is locked with a token." }), h("br"), form)));
    document.body.appendChild(screen);
    form.inputs.token.focus();
  }

  // ---- theme -----------------------------------------------------------------------------------

  var THEME_KEY = "kryta.theme";

  function currentTheme() {
    return document.documentElement.getAttribute("data-theme") ||
      (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  }

  function applyTheme(theme) {
    if (theme) document.documentElement.setAttribute("data-theme", theme);
    else document.documentElement.removeAttribute("data-theme");
  }

  // ---- the rail --------------------------------------------------------------------------------

  var railItems = {};

  function buildRail() {
    var rail = clear(document.getElementById("rail"));
    railItems = {};
    rail.appendChild(h("a", { class: "brand", href: "#/" + areas[0].id }, h("div", { class: "mark", text: "K" }), h("span", { text: "Kryta Control" })));
    rail.appendChild(h("button", { class: "find", type: "button", on: { click: openPalette } }, "Find", h("kbd", { text: "Ctrl K" })));
    var group = null;
    areas.forEach(function (area) {
      if (area.group !== group) {
        group = area.group;
        rail.appendChild(h("h6", { text: group }));
      }
      var count = h("span", { class: "count", hidden: true });
      var item = h("a", { class: "item", href: "#/" + area.id }, area.label, count);
      railItems[area.id] = { item: item, count: count };
      rail.appendChild(item);
      if (area.count) refreshCount(area);
    });
    var themeButton = h("button", { type: "button", on: { click: function () {
      var next = currentTheme() === "dark" ? "light" : "dark";
      applyTheme(next);
      store(window.localStorage, THEME_KEY, next);
      themeButton.textContent = next === "dark" ? "Light theme" : "Dark theme";
    } } }, currentTheme() === "dark" ? "Light theme" : "Dark theme");
    rail.appendChild(h("div", { class: "foot" }, themeButton));
  }

  function refreshCount(area) {
    area.count().then(function (count) {
      var slot = railItems[area.id].count;
      slot.hidden = count === null || count === undefined;
      slot.textContent = fmt.number(count);
    }).catch(function () { /* the page itself reports a failure; the count just stays hidden */ });
  }

  /* An area calls this after it changed something its menu count shows. */
  Kryta.recount = function (id) {
    var area = areas.filter(function (one) { return one.id === id; })[0];
    if (area && area.count) refreshCount(area);
    paletteItems = null;
  };

  // ---- the router ------------------------------------------------------------------------------

  var timers = [];
  var generation = 0;

  /* "#/players/abc?account=3" -> {id: "players", parts: ["abc"], query: {account: "3"}} */
  function parseRoute() {
    var hash = window.location.hash.replace(/^#\/?/, "");
    var pieces = hash.split("?");
    var parts = pieces[0].split("/").filter(Boolean).map(decodeURIComponent);
    var query = {};
    (pieces[1] || "").split("&").filter(Boolean).forEach(function (pair) {
      var both = pair.split("=");
      query[decodeURIComponent(both[0])] = decodeURIComponent(both[1] || "");
    });
    return { id: parts[0] || areas[0].id, parts: parts.slice(1), query: query };
  }

  async function navigate() {
    var route = parseRoute();
    var area = areas.filter(function (one) { return one.id === route.id; })[0];
    var mine = ++generation;
    timers.forEach(clearInterval);
    timers = [];
    while (overlays.length) overlays[overlays.length - 1]();
    for (var id in railItems) railItems[id].item.classList.toggle("on", id === route.id);

    var main = clear(document.getElementById("main"));
    var title = h("h1", { text: area ? area.label : "Not found" });
    var sub = h("span", { class: "sub", text: area ? area.hint || "" : "" });
    var actions = h("span", null);
    var body = h("div", null, ui.skeleton(6));
    main.appendChild(h("div", { class: "view" },
      h("header", { class: "top" }, title, sub, h("span", { class: "spacer" }), actions), body));
    document.title = (area ? area.label : "Not found") + " - Kryta Control";
    window.scrollTo(0, 0);

    if (!area) {
      clear(body).appendChild(ui.empty("No such page", "Nothing is at #/" + route.id + ".", ui.link("Go to the overview", "#/" + areas[0].id, "primary")));
      return;
    }
    var view = {
      route: route,
      alive: function () { return mine === generation; },
      /* Replace the page body (the skeleton, or what was there). */
      body: function () { if (view.alive()) append(clear(body), Array.prototype.slice.call(arguments)); },
      /* Set the header: a sub-title crumb, the hint line, and action buttons. */
      head: function (options) {
        if (!view.alive()) return;
        clear(title).appendChild(document.createTextNode(area.label));
        if (options.crumb) {
          clear(title).appendChild(h("a", { href: "#/" + area.id, text: area.label }));
          title.appendChild(h("span", { class: "crumb", text: " / " }));
          title.appendChild(document.createTextNode(options.crumb));
        }
        if (options.sub !== undefined) sub.textContent = options.sub;
        if (options.actions) append(clear(actions), options.actions);
      },
      /* Call fn every ms while this page is the one shown. */
      every: function (ms, fn) {
        if (view.alive()) timers.push(setInterval(function () { if (!document.hidden) fn(); }, ms));
      },
      reload: navigate
    };
    try {
      await area.render(view, route);
    } catch (error) {
      if (view.alive() && error.status !== 401) {
        clear(body).appendChild(ui.card(null, ui.empty("This page could not be loaded", error.message, ui.button("Try again", navigate, "primary"))));
      }
    }
  }

  /* Go to an address; when it is already the address, draw it again. */
  Kryta.go = function (hash) {
    if (window.location.hash === hash) navigate(); else window.location.hash = hash;
  };

  // ---- the palette (Ctrl+K) --------------------------------------------------------------------

  var paletteItems = null;
  var paletteOpen = false;

  async function loadPaletteItems() {
    var items = areas.map(function (area) {
      return { label: area.label, hint: "page", href: "#/" + area.id, words: area.group + " " + (area.hint || "") };
    });
    var found = await Promise.all(areas.filter(function (area) { return area.find; }).map(function (area) {
      return area.find().catch(function () { return []; });
    }));
    found.forEach(function (list) { items = items.concat(list); });
    return items;
  }

  function openPalette() {
    if (paletteOpen) return;
    paletteOpen = true;
    var items = areas.map(function (area) { return { label: area.label, hint: "page", href: "#/" + area.id, words: area.group }; });
    var shown = [];
    var selected = 0;
    var list = h("ul", { role: "listbox" });
    var input = h("input", { type: "text", placeholder: "Jump to a page, an account, a player or a map", "aria-label": "Find", autocomplete: "off", spellcheck: "false" });
    var close = overlay("high", h("div", { class: "palette", role: "dialog", "aria-label": "Find" }, input, list), function () { paletteOpen = false; });

    function choose(item) {
      close();
      Kryta.go(item.href);
    }
    function draw() {
      var typed = input.value.trim();
      shown = items.filter(function (item) { return !typed || ui.matches(typed, [item.label, item.hint, item.words]); }).slice(0, 40);
      selected = Math.min(selected, Math.max(shown.length - 1, 0));
      clear(list);
      if (!shown.length) list.appendChild(h("div", { class: "none", text: "Nothing matches." }));
      shown.forEach(function (item, index) {
        list.appendChild(h("li", { class: index === selected ? "on" : null, role: "option", on: {
          click: function () { choose(item); },
          mousemove: function () { if (selected !== index) { selected = index; draw(); } }
        } }, h("span", { text: item.label }), h("span", { class: "kind", text: item.hint })));
      });
      var active = list.querySelector("li.on");
      if (active) active.scrollIntoView({ block: "nearest" });
    }
    input.addEventListener("input", function () { selected = 0; draw(); });
    input.addEventListener("keydown", function (event) {
      if (event.key === "ArrowDown") { selected = Math.min(selected + 1, shown.length - 1); draw(); event.preventDefault(); }
      else if (event.key === "ArrowUp") { selected = Math.max(selected - 1, 0); draw(); event.preventDefault(); }
      else if (event.key === "Enter" && shown[selected]) { choose(shown[selected]); event.preventDefault(); }
    });
    draw();
    input.focus();
    (paletteItems ? Promise.resolve(paletteItems) : loadPaletteItems()).then(function (loaded) {
      paletteItems = loaded;
      items = loaded;
      if (paletteOpen) draw();
    });
  }

  document.addEventListener("keydown", function (event) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      openPalette();
    }
  });

  // ---- start -----------------------------------------------------------------------------------

  Kryta.h = h;
  Kryta.esc = esc;
  Kryta.api = api;
  Kryta.ui = ui;
  Kryta.fmt = fmt;
  Kryta.clear = clear;

  applyTheme(stored(window.localStorage, THEME_KEY));
  window.addEventListener("hashchange", navigate);
  document.addEventListener("DOMContentLoaded", function () {
    buildRail();
    navigate();
  });
})();
