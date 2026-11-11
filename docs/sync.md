# Sync

Copies of a database sync peer to peer over [iroh](https://iroh.computer),
as members of a sync chain. Join one with a one-time invite, then sync:

```bash,no_run
datum sync chain invite            # on the first copy: starts a chain, prints `datum sync chain join <LINK>`
datum sync chain join <LINK>       # on the second: it joins the chain, and they sync
datum sync                         # sync with every member once
datum sync status                  # how this copy stands with each member
datum sync chain list              # list them; `datum sync chain expel <WHO>` removes one
```

A chain with no admin is open: any member may invite, expel or rename
another. `datum sync admin add <KEYFILE>` (or a datum.day passkey) locks
it, and from then on those acts take the admin key; removing the last
admin takes `--allow-unlock`, and opens it again. The link is a datum.day
join link (`https://datum.day/sync/join#<ticket>`): opened in a browser, it
joins that browser's copy instead, and with `?db=<name>` (the database name)
before its `#`, `https://datum.day/sync/join?db=<name>#<ticket>`, it joins as
the database of that name, beside the browser's own record: an agent shares
its own record this way (`?db=claude`), so its person can watch it. Only
members are served, and the first command after a member is added warns
about it once. Every
change to an entry is an op signed by this copy's endpoint key (which is
also its iroh identity), and two copies that hold the same ops hold the
same entries. `datum sync help` lists the whole tree.

On datum.day a person can sign in to sync with a passkey (the Sync widget's
«Sign in with a passkey»), where the browser and passkey give WebAuthn's PRF:
passkeys in iCloud Keychain (iOS and iPadOS 18.4, macOS 15.4 and later) and
Google Password Manager do, and Windows Hello's stay on the one device. The
passkey's PRF secret, asked with the fixed salt `datum.day account v1`, is the
same on every device the passkey is on, and an account key is derived from it
(HKDF-SHA256, info `datum account key v1`), held in the tab until it signs out
or reloads, never stored. A chain started while signed in has that key as its
admin, so it is locked and the account's: while signed in, the account signs
its invites, expels and renames, and signed out they take an admin key as any
locked chain's do. A device signed in to the same account joins that chain
with no invite from another device: it signs an invite with the account key
and redeems it itself, and a member that lacks the invite writes it as the
admin's before it admits the device. Like any join, it waits until a member is
online. Without PRF, invites are the way in.

Per key the op with the later hybrid logical clock wins. Concurrent edits
of one key, or a delete against a concurrent edit, leave the entry's
`conflicts` naming each side — the same on every copy that holds both — which
`datum check` and `datum conflicts` show until an edit that saw both settles
it (`datum conflicts resolve`).

A copy of the file at another path mints its own key the first time it
signs, so `cp a.db b.db` gives two copies that sync. Two copies found
signing with one key are refused by name; `datum sync chain rekey` gives one of
them a new key (and it joins the chain again).

A copy a newer datum migrated while the rest of its chain stayed behind is
brought back by the chain's datum with `datum sync chain reenter`: it keeps
the file whole beside it, refetches the chain under the same key, and replays
what it made that the chain never had. A change the older datum cannot hold
is named and stays in that backup.
