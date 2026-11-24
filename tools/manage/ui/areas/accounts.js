/* Accounts: the login identities - the list, and one account with its players. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  function stateBadge(account) {
    return ui.badge(account.active ? "active" : "disabled", account.active ? "ok" : "plain", true);
  }

  function eulaText(account) {
    return account.eula ? "accepted (" + account.eula + ")" : "not accepted";
  }

  function newAccount() {
    ui.formModal({
      title: "New account",
      intro: "A login identity. Leave the portal id and token blank to have them generated.",
      submit: "Create account",
      fields: [
        { name: "username", label: "Login name", placeholder: "name@example.com", full: true },
        { name: "user_id", label: "Portal user id", placeholder: "generated when blank" },
        { name: "token", label: "Portal token", type: "password", placeholder: "generated when blank" }
      ],
      onSubmit: async function (values) {
        var data = await api.post("/accounts", values);
        ui.toast("Account " + data.account.username + " created.");
        Kryta.recount("accounts");
        Kryta.go("#/accounts/" + data.account.id);
      }
    });
  }

  async function list(view) {
    var data = await api.get("/accounts");
    var typed = "", state = "all";
    var table = ui.table({
      columns: [
        { label: "Login name", sort: function (row) { return row.username.toLowerCase(); },
          value: function (row) { return [h("b", { text: row.username }), h("div", { class: "sub", text: "#" + row.id })]; } },
        { label: "State", sort: function (row) { return row.active ? 0 : 1; }, value: stateBadge },
        { label: "EULA", value: eulaText },
        { label: "Players", num: true, sort: function (row) { return row.players; }, value: function (row) { return row.players; } },
        { label: "", class: "actions", value: function (row) { return ui.link("Open", "#/accounts/" + row.id, "small"); } }
      ],
      onRow: function (row) { Kryta.go("#/accounts/" + row.id); },
      empty: data.accounts.length
        ? { title: "No account matches", text: "Change the search or the filter." }
        : { title: "No account exists yet", text: "Create the first one to let somebody log in.", action: ui.button("New account", newAccount, "primary") }
    });
    function filter() {
      table.update(data.accounts.filter(function (row) {
        return (state === "all" || (state === "active") === row.active) &&
          (!typed || ui.matches(typed, [row.username, row.user_id, "#" + row.id]));
      }));
    }
    filter();
    view.head({ actions: ui.button("New account", newAccount, "primary") });
    view.body(
      ui.toolbar(
        ui.search("Search by login name or id", function (value) { typed = value; filter(); }),
        ui.select([{ value: "all", label: "All" }, { value: "active", label: "Active" }, { value: "disabled", label: "Disabled" }],
          function (value) { state = value; filter(); }, state, "State")),
      ui.card(null, table, { flush: true }));
  }

  async function detail(view, id) {
    var data = await api.get("/accounts/" + encodeURIComponent(id));
    var account = data.account;
    var base = "/accounts/" + account.id;
    view.head({ crumb: account.username, sub: "account #" + account.id });

    var tokenValue = h("code", { text: account.token_masked || "(none)" });
    var reveal = ui.button("Reveal", async function () {
      if (reveal.textContent === "Hide") {
        tokenValue.textContent = account.token_masked;
        reveal.textContent = "Reveal";
        return;
      }
      await ui.act(async function () {
        tokenValue.textContent = (await api.get(base + "/token")).token;
        reveal.textContent = "Hide";
      });
    }, "small");

    var identity = ui.form({
      submit: "Save account",
      fields: [
        { name: "username", label: "Login name", value: account.username, full: true },
        { name: "user_id", label: "Portal user id", value: account.user_id },
        { name: "token", label: "Portal token", type: "password", placeholder: "leave blank to keep the current one" }
      ],
      onSubmit: async function (values) {
        await api.post(base + "/update", values);
        ui.toast("Account saved.");
        view.reload();
      }
    });

    async function toggle(path, body, done) {
      if (await ui.act(function () { return api.post(base + path, body); }, done)) view.reload();
    }

    var players = data.players === null
      ? ui.empty("No character database", "CharacterDatabaseInfo is empty in manage.json, so this account's players cannot be read.")
      : ui.table({
          columns: [
            { label: "Player", value: function (row) { return h("b", { text: row.name }); } },
            { label: "Level", num: true, value: function (row) { return row.level; } },
            { label: "Campaign", value: function (row) { return row.campaign; } },
            { label: "Professions", value: function (row) { return row.professions; } },
            { label: "Where", value: function (row) { return row.outpost; } }
          ],
          rows: data.players,
          onRow: function (row) { Kryta.go("#/players/" + row.uuid); },
          empty: { title: "No player on this account yet", text: "Create one from the Players page.",
            action: ui.link("Open players", "#/players?account=" + account.id, "small") }
        });

    view.body(ui.grid("g3",
      h("div", { class: "stack" },
        ui.card("Identity", [identity, h("br"),
          ui.kv([["Portal token", [tokenValue, " ", reveal]]])]),
        ui.card(["Players", h("span", { class: "muted", text: String(account.players) }), h("span", { class: "spacer" }),
          ui.link("All of them", "#/players?account=" + account.id, "small")], players, { flush: true })),
      h("div", { class: "stack" },
        ui.card("Login state", [
          ui.check(stateBadge(account), account.active ? "May start a new login" : "Refused at the next login",
            "Deactivating does not end a session already in progress."),
          ui.check(ui.badge(account.eula ? "accepted" : "not accepted", account.eula ? "ok" : "warn", true),
            "EULA: " + eulaText(account), account.eula ? "The client is not asked again." : "The client is asked to accept it."),
          h("div", { class: "form-foot" },
            ui.button(account.active ? "Deactivate" : "Activate", function () {
              toggle("/active", { active: !account.active }, account.active ? "Account deactivated." : "Account activated.");
            }),
            ui.button(account.eula ? "Ask the client again" : "Record acceptance", function () {
              toggle("/eula", { accepted: !account.eula }, account.eula ? "The EULA version was cleared." : "EULA acceptance recorded.");
            }))]),
        ui.card("Remove", [
          h("p", { class: "muted", text: "Removes this login account only. Its player records are kept and can be removed from their own pages." }),
          h("div", { class: "form-foot" }, ui.button("Remove account", async function () {
            var sure = await ui.confirm({ title: "Remove " + account.username + "?",
              text: "The login account is deleted. Its " + account.players + " player(s) are kept.", label: "Remove account", danger: true });
            if (!sure) return;
            if (await ui.act(function () { return api.post(base + "/delete"); }, "Account " + account.username + " removed.")) {
              Kryta.recount("accounts");
              Kryta.go("#/accounts");
            }
          }, "danger"))]))));
  }

  Kryta.area({
    id: "accounts",
    label: "Accounts",
    group: "People",
    hint: "who may log in",
    count: async function () { return (await api.get("/accounts")).accounts.length; },
    find: async function () {
      return (await api.get("/accounts")).accounts.map(function (account) {
        return { label: account.username, hint: "account", href: "#/accounts/" + account.id };
      });
    },
    render: function (view, route) {
      return route.parts[0] ? detail(view, route.parts[0]) : list(view);
    }
  });
})();
