/* Players: the characters - the list across accounts, and one player with what it owns. */
(function () {
  "use strict";
  var h = Kryta.h, ui = Kryta.ui, api = Kryta.api;

  function dataBadge(check) {
    if (!check) return ui.badge("not read", "plain");
    return ui.badge(check.text, check.sound ? "ok" : "bad", true);
  }

  function newCharacter(data, account) {
    var choices = data.choices;
    var named = function (list) { return list.map(function (one) { return { value: one.id, label: one.name }; }); };
    ui.formModal({
      title: "New character",
      wide: true,
      intro: "Written with the creation lists' starting items and its first map unlocked. PvP creation is unavailable: the world pack names no PvP starting map.",
      submit: "Create character",
      fields: [
        { name: "account", label: "Account", type: "select", value: account || undefined,
          options: data.accounts.map(function (one) { return { value: one.id, label: one.username }; }) },
        { name: "name", label: "Name", placeholder: "First Last", hint: "3 to 19 English letters, a first name and a surname" },
        { name: "campaign", label: "Campaign", type: "select", options: named(choices.campaigns) },
        { name: "profession", label: "Profession", type: "select", options: named(choices.professions),
          hint: "Assassin and Ritualist need Factions; Paragon and Dervish need Nightfall" },
        { name: "sex", label: "Sex", type: "select", options: [{ value: 0, label: "Male" }, { value: 1, label: "Female" }] }
      ].concat(choices.appearance.map(function (field) {
        return { name: field.name, label: field.label + " (0-" + field.max + ")", type: "number", min: 0, max: field.max, value: field.value };
      })),
      onSubmit: async function (values) {
        var created = await api.post("/players", values);
        ui.toast(values.name + " created.");
        Kryta.recount("players");
        Kryta.go("#/players/" + created.uuid);
      }
    });
  }

  async function list(view, route) {
    var data = await api.get("/players");
    var typed = "", account = route.query.account || "all";
    var create = function () { newCharacter(data, account === "all" ? null : account); };
    var canCreate = data.configured && data.accounts.length;
    var table = ui.table({
      columns: [
        { label: "Name", sort: function (row) { return row.name.toLowerCase(); }, value: function (row) { return h("b", { text: row.name }); } },
        { label: "Account", sort: function (row) { return row.account || ""; }, value: function (row) {
          return row.account ? h("a", { href: "#/accounts/" + row.account_id, text: row.account }) : h("span", { class: "muted", text: "account not recorded" });
        } },
        { label: "Profession", value: function (row) { return row.professions; } },
        { label: "Level", num: true, sort: function (row) { return row.level; }, value: function (row) { return row.level; } },
        { label: "Campaign", value: function (row) { return row.campaign; } },
        { label: "Where", value: function (row) { return row.outpost; } },
        { label: "Items", num: true, value: function (row) { return row.data ? row.data.items : "-"; } },
        { label: "Data", sort: function (row) { return row.data && row.data.sound ? 1 : 0; }, value: function (row) { return dataBadge(row.data); } }
      ],
      onRow: function (row) { Kryta.go("#/players/" + row.uuid); },
      empty: !data.configured
        ? { title: "No character database", text: "CharacterDatabaseInfo is empty in manage.json, so there is nothing to list." }
        : data.players.length
          ? { title: "No player matches", text: "Change the search or the account." }
          : { title: "No character exists yet", text: data.accounts.length ? "Create one here, or log in with a client and make one." : "Create an account first; a character belongs to one.",
              action: data.accounts.length ? ui.button("New character", create, "primary") : ui.link("Open accounts", "#/accounts", "primary") }
    });
    function filter() {
      table.update(data.players.filter(function (row) {
        return (account === "all" || String(row.account_id) === String(account)) &&
          (!typed || ui.matches(typed, [row.name, row.account, row.professions, row.outpost]));
      }));
    }
    filter();
    view.head({ actions: canCreate ? ui.button("New character", create, "primary") : null });
    view.body(
      ui.toolbar(
        ui.search("Search by name, account or place", function (value) { typed = value; filter(); }),
        ui.select([{ value: "all", label: "Every account" }].concat(data.accounts.map(function (one) {
          return { value: one.id, label: one.username };
        })), function (value) { account = value; filter(); }, account, "Account")),
      ui.card(null, table, { flush: true }));
  }

  async function detail(view, uuid) {
    var data = await api.get("/players/" + encodeURIComponent(uuid));
    var player = data.player;
    var base = "/players/" + encodeURIComponent(player.uuid);
    view.head({ crumb: player.name, sub: player.professions + ", level " + player.level });

    var position = player.position
      ? "x " + player.position.x.toFixed(2) + ", y " + player.position.y.toFixed(2) + ", plane " + player.position.plane + " in " + player.position.map
      : "never saved - the player has not been placed since it was played";
    var look = player.sex + ", " + player.appearance.map(function (field) { return field.name + " " + field.value; }).join(", ");

    var rename = ui.form({
      submit: "Rename",
      fields: [{ name: "name", label: "Name", value: player.name, full: true }],
      onSubmit: async function (values) {
        await api.post(base + "/rename", values);
        ui.toast("Renamed to " + values.name + ".");
        Kryta.recount("players");
        view.reload();
      }
    });
    var level = ui.form({
      submit: "Save level",
      fields: [{ name: "level", label: "Level (1 to 20)", type: "number", min: 1, max: 20, value: player.level, full: true }],
      onSubmit: async function (values) {
        await api.post(base + "/level", values);
        ui.toast("Level set to " + values.level + ".");
        view.reload();
      }
    });

    var items = data.items_error
      ? ui.empty("The item rows could not be read", data.items_error)
      : ui.table({
          columns: [
            { label: "Item", value: function (row) { return ui.itemName(row.name); } },
            { label: "Type", num: true, value: function (row) { return row.item_type === 0 ? ui.badge("0 - none", "bad") : row.item_type; } },
            { label: "Bag", num: true, value: function (row) { return row.bag; } },
            { label: "Slot", num: true, value: function (row) { return row.slot; } },
            { label: "Quantity", num: true, value: function (row) { return row.quantity; } },
            { label: "File id", num: true, value: function (row) { return h("code", { text: row.file_id }); } },
            { label: "Model", num: true, value: function (row) { return row.model_id; } }
          ],
          rows: data.items,
          empty: { title: "The player carries nothing" }
        });

    view.body(
      data.data.sound ? null : ui.banner(data.data.text + ". A client that loads this character can crash; remove the character or repair its rows.", "bad"),
      ui.grid("g3",
        h("div", { class: "stack" },
          ui.card(["Identity", h("span", { class: "spacer" }), dataBadge(data.data)], ui.kv([
            ["Player id", h("code", { text: player.uuid })],
            ["Account", data.account ? h("a", { href: "#/accounts/" + data.account.id, text: data.account.username }) : "not in the account table; the player row is kept"],
            ["Campaign", player.campaign],
            ["Professions", player.professions + " (primary " + player.primary + ")"],
            ["Professions unlocked", player.unlocked_professions.length ? player.unlocked_professions.join(", ") : "none"],
            ["Level", String(player.level)],
            ["Experience", player.experience === null ? "not stated by the row" : String(player.experience)],
            ["Gold", String(player.gold)],
            ["PvP", player.pvp ? "yes" : "no"],
            ["Last outpost", player.outpost],
            ["Position", position],
            ["Appearance", look]
          ])),
          ui.card(["Inventory and equipment", h("span", { class: "muted", text: data.items.length + " item rows" })], items, { flush: true }),
          ui.card(["Unlocked maps", h("span", { class: "muted", text: String(data.unlocks.length) })],
            data.unlocks.length
              ? h("div", null, data.unlocks.map(function (map) { return [ui.badge(map.map_id + (map.name ? " " + map.name : ""), "plain"), " "]; }))
              : ui.empty("No unlocked map yet", "A map is unlocked when the player first enters it (R12)."))),
        h("div", { class: "stack" },
          ui.banner("Changes are not synchronised with an online player. Log the player out before editing or deleting it."),
          ui.card("Rename", rename),
          ui.card("Level", level),
          ui.card("Remove", [
            h("p", { class: "muted", text: "Removes the player's row and the records that are its own: its items and its unlocked maps." }),
            h("div", { class: "form-foot" }, ui.button("Remove player", async function () {
              var sure = await ui.confirm({ title: "Remove " + player.name + "?",
                text: "The character, its " + data.items.length + " item row(s) and its unlocked maps are deleted.", label: "Remove player", danger: true });
              if (!sure) return;
              if (await ui.act(function () { return api.post(base + "/delete"); }, player.name + " removed.")) {
                Kryta.recount("players");
                Kryta.go("#/players");
              }
            }, "danger"))]))));
  }

  Kryta.area({
    id: "players",
    label: "Players",
    group: "People",
    hint: "characters and what they own",
    count: async function () {
      var data = await api.get("/players");
      return data.configured ? data.players.length : null;
    },
    find: async function () {
      return (await api.get("/players")).players.map(function (player) {
        return { label: player.name, hint: "player" + (player.account ? " of " + player.account : ""), href: "#/players/" + player.uuid };
      });
    },
    render: function (view, route) {
      return route.parts[0] ? detail(view, route.parts[0]) : list(view, route);
    }
  });
})();
