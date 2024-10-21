import { timingView } from "../views/datumViews";

export async function occurredFields(db: PouchDB.Database): Promise<string[]> {
  const fields = await db.query(timingView.name, {
    reduce: true,
    group: true,
    startkey: ["occur"],
    endkey: ["occurZ"],
    group_level: 2,
  });
  const rows = fields.rows;
  // first row is total count with field=null
  rows.shift();
  return rows.map((row) => row.key[1]);
}
