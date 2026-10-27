# Sync

Copies of a database sync peer to peer over [iroh](https://iroh.computer),
as members of a sync chain. Join one with a one-time invite, then sync:

```bash,no_run
datum sync chain invite            # on the first copy: starts a chain, prints `datum sync chain connect <LINK>`
datum sync chain connect <LINK>    # on the second: it joins the chain, and they sync
datum sync                         # sync with every member once
datum sync status                  # how this copy stands with each member
datum sync chain list              # list them; `datum sync chain expel <WHO>` removes one
```

A chain with no admin is open: any member may invite, expel or rename
another. `datum sync admin add <KEYFILE>` (or a datum.day passkey) locks
it, and from then on those acts take the admin key; removing the last
admin takes `--allow-unlock`, and opens it again. The link is a datum.day
join link (`https://datum.day/sync/join#<ticket>`): opened in a browser, it
joins that browser's copy instead. Only members are served,
and the first command after a member is added warns about it once. Every
change to an entry is an op signed by this copy's endpoint key (which is
also its iroh identity), and two copies that hold the same ops hold the
same entries. `datum sync help` lists the whole tree.

Per key the op with the later hybrid logical clock wins. Concurrent edits
of one key, or a delete against a concurrent edit, leave the entry's
`conflicts` naming each side — the same on every copy that holds both — which
`datum check` and `datum conflicts` show until an edit that saw both settles
it (`datum conflicts resolve`).

A copy of the file at another path mints its own key the first time it
signs, so `cp a.db b.db` gives two copies that sync. Two copies found
signing with one key are refused by name; `datum sync chain rekey` gives one of
them a new key (and its peers pair with it again).
