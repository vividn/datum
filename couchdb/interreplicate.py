#!/usr/bin/env python3
from time import sleep
from urllib.parse import urlparse

from cloudant.client import CouchDB
from cloudant.replicator import Replicator
from cloudant.document import Document

username = "admin"
password = "***REMOVED***"

couch = CouchDB(username, password, url="http://localhost:5984")
w_couch = CouchDB(username, password, url="http://couchdb.example.com:5984")


def resync_replicator(local_couch: CouchDB, server_couch: CouchDB):
    remote_db = server_couch["master_replicator"]

    if "_replicator" in local_couch.all_dbs():
        local_couch.delete_database("_replicator")
    local_db = local_couch.create_database("_replicator")

    replicator = Replicator(local_couch)
    repl_doc: Document = replicator.create_replication(
        remote_db, local_db, continuous=False,
    )
    id = repl_doc["_id"]

    repl_state = None
    while repl_state != "completed":
        repl_state = replicator.replication_state(id)
        if repl_state == "error":
            raise RuntimeError("_replicator replication failed")
        sleep(0.1)

    repl_doc.fetch()
    repl_doc.delete()


def replicate_all_dbs(local_couch: CouchDB, server_couch: CouchDB):
    all_dbs = set(
        filter(
            lambda database_name: not (database_name.startswith("_")),
            local_couch.all_dbs() + server_couch.all_dbs(),
        )
    )
    for db_name in all_dbs:
        bidirectional_replication(db_name, local_couch, server_couch)


def bidirectional_replication(
    db_name: str,
    local_couch: CouchDB,
    server_couch: CouchDB,
    local_replicator_repl=False,
):
    replicator = Replicator(local_couch)
    existing_repl_ids = map(lambda doc: doc["_id"], replicator.list_replications())

    db_a = local_couch.create_database(db_name, throw_on_exists=False)

    if local_replicator_repl:
        db_b = local_couch.create_database("_replicator", throw_on_exists=False)
    else:
        db_b = server_couch.create_database(db_name, throw_on_exists=False)

    repl_ids = generate_ids(db_name, server_couch.server_url, local_replicator_repl)

    if repl_ids["to_id"] not in existing_repl_ids:
        replicator.create_replication(
            source_db=db_a,
            target_db=db_b,
            repl_id=repl_ids["to_id"],
            create_target=True,
            continuous=True,
        )

    if repl_ids["from_id"] not in existing_repl_ids:
        replicator.create_replication(
            source_db=db_b,
            target_db=db_a,
            repl_id=repl_ids["from_id"],
            create_target=True,
            continuous=True,
        )


def create_local_replicator_repl(local_couch: CouchDB):
    bidirectional_replication(
        "master_replicator", local_couch, local_couch, local_replicator_repl=True
    )


def generate_ids(db_name: str, server_url: str, local_replicator_repl=False):
    if local_replicator_repl:
        target = "repl_db"
    else:
        target = urlparse(server_url).hostname

    return {
        "to_id": f"{db_name}__to__{target}",
        "from_id": f"{db_name}__from__{target}",
    }


if __name__ == "__main__":
    couch.connect()
    w_couch.connect()

    resync_replicator(couch, w_couch)
    replicate_all_dbs(couch, w_couch)
    create_local_replicator_repl(couch)

    couch.disconnect()
    w_couch.disconnect()
