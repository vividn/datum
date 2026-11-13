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

A member writes and relays: it serves every op it holds. An invite with
`--reader` makes a reader, served the record and writing nothing; one with
`--writer` makes a writer, which writes entries as a member does and, like a
reader, serves only its own ops, so only a member ever relays. A writer
changes only the entries it created and writes no alias or other entry about
the database; filtered to fields (`--fields a,b` or `--kit <KIT>`), it writes
only those and their specs, and with `--fields ''` it is sent the chain's
records alone, for a sensor or an agent that only logs. `datum sync chain
role <WHO> member|reader|writer` changes a member's role.

A reader's or writer's filter may also hold a window: `--window 1d` passes
the entries of the last day, back to the hour, and for each field the entry
that set its state at the window's start, so a field that last changed a
week ago still reads as what it is. `--specs` passes every spec and alias
whatever else the filter says, and neither is windowed. The window is judged
again at every sync: an entry that aged out is dropped from the filtered
copy, unless it holds an op of the copy's own.

An agent with no persistent storage comes back from a code. `datum sync
chain agent --level full|light|write` (datum.day's «Add agent») makes a new
device key and a standing admission for it, an admin act in a locked chain,
and prints `datum-agent:…`, which holds that key and a member to dial.
`datum sync restore <CODE>` in a fresh home makes it this install's device
key and admits a new endpoint by it, at the level's role and filter: full a
reader (a member with `--write`), light a reader of the last `--window` and
every spec and alias (a writer with `--write`), write a writer sent no
entries, limited to `--fields` where named. Each restore is a fresh copy of
the same device, one row of the grid, and the endpoints it held before are
revoked as it is admitted; what they wrote stands. Whoever holds the code is
that agent until it expires (`--expires`) or its copy is expelled (`datum
sync chain expel <ENDPOINT>`, «Remove» on its row), after which it admits
nothing.

A device you have lost, or that someone else may hold, should be unjoined
(`datum sync chain expel <WHO> --unjoin`, the Sync widget's «Unjoin»), which
sets aside everything it wrote, not only removed. Removing it trusts the
device's own clock for what it wrote before the removal, so until every member
has heard of the removal, it could still send writes dated earlier through
one that has not, and they would stand.

On datum.day a person can sign in to sync with a passkey (the Sync widget's
«Sign in with a passkey»), where the browser and passkey give WebAuthn's PRF:
passkeys in iCloud Keychain (iOS and iPadOS 18.4, macOS 15.4 and later) and
Google Password Manager do, and Windows Hello's stay on the one device. The
passkey's PRF secret, asked with the fixed salt `datum.day account v1`, is the
same on every device the passkey is on, and an account key is derived from it
(HKDF-SHA256, info `datum account key v1`). A chain started while signed in
has that key as its admin, so it is locked and the account's. No device keeps
that key: it keeps a second key, which reads the account's record and signs
nothing, so each invite, expel and rename in the account's chain asks for the
passkey again, naming what it approves and in which chain. A stolen device
that is signed in is no admin. A device signed in to the same account joins that chain
with no invite from another device: it signs an invite with the account key
and redeems it itself, and a member that lacks the invite writes it as the
admin's before it admits the device. Like any join, it waits until a member is
online. Without PRF, invites are the way in.

Which chains to join, a device signed in learns from the account's record:
a small record published under the account key to n0's pkarr relay, which
anyone can fetch and only the account's devices can read, since it is
encrypted with a second key derived from the passkey's secret. It lists each
chain the account is an admin of, the database it is in and a member or a few
to reach it through. Signing in reads it and joins each chain this device is
not in, into a database of the same name (or `<name>-2` where that one holds
data of its own); a device publishes it again with its own chains whenever
the passkey is asked: at a sign-in, and at each approval, a chain's start
among them. It holds about ten chains. The relay keeps it for 7 days after
it was signed, and sending it again does not extend that: only signing it
again does. So from day 5 the Sync widget of a signed-in tab says when the
record lapses, with a button that asks your passkey once and signs it again.
Once it has lapsed, the widget says so wherever a chain on this device is
your account's, and the same button publishes it again, as does the next
sign-in or approval on any device of the account.

A terminal signs in by `datum sync login`: it prints a datum.day link, and
opens it where it can. Open it on this computer or on a phone, sign in with
the passkey there, and the page sends the account to the waiting CLI over
the sync relays, sealed to this install's device key, so a CLI on a server
signs in from a phone's browser. The CLI joins the account's chains as a
browser does, and keeps in `account` in its data directory, readable by its
user alone, what reads the account's record and nothing that signs: an admin
act there prints a link for the passkey to approve. `datum sync status` says
which account it is signed in to, and `datum sync logout` forgets it. Only
open a login link from a `datum sync login` you ran yourself: whoever
printed it joins the account's chains.

A device signed in by a datum before `datum 3.2.2899-beta` (as `datum
--version` prints it) kept the passkey's PRF output, which signs for the
account. A newer datum rewrites the `account` file to the form that signs
nothing as it first reads it, but the old bytes can outlive the file in freed
disk blocks, snapshots and backups, and a `DATUM_ACCOUNT` set to it in a shell
profile or script still signs: take it out. The PRF output is the same for
the life of the passkey, so if such a device is lost or stolen, in each chain
the account is an admin of add a new passkey as admin (the Sync widget's «Add
passkey» on datum.day, which runs `datum sync admin add <PASSKEY-JSON>`), then
remove the old account key, the admin labelled `account`: `datum sync admin
remove account`. A device signed in only by 3.2.2899-beta or later holds
nothing that signs.

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
