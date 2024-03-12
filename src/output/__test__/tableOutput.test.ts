import * as connectDbModule from "../../auth/connectDb";
import { connectDb } from "../../auth/connectDb";
import { addCmd } from "../../commands/addCmd";
import { endCmd } from "../../commands/endCmd";
import { occurCmd } from "../../commands/occurCmd";
import { setupCmd } from "../../commands/setupCmd";
import { startCmd } from "../../commands/startCmd";
import { switchCmd } from "../../commands/switchCmd";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { Show } from "../../input/outputArgs";
import {
  deterministicHumanIds,
  pass,
  resetTestDb,
  restoreNow,
  setNow,
} from "../../__test__/test-utils";
import { tableOutput } from "../tableOutput";

describe("tableOutput", () => {
  deterministicHumanIds();
  const docs: EitherDocument[] = [];
  beforeAll(async () => {
    // tmp db to generate docs easily
    const db = connectDb({ db: "test_table_output" });
    await resetTestDb(db);
    jest.spyOn(connectDbModule, "connectDb").mockReturnValue(db);

    await setupCmd("");
    setNow("2024-03-11 17:45");
    docs.push(await occurCmd("field1 foo=bar"));
    setNow("+1");
    docs.push(await switchCmd("emotion happy 10 foo=baz"));
    setNow("+1");
    docs.push(await occurCmd("field2 foo=foo -t 10:00"));
    setNow("+1");
    docs.push(await addCmd("field2 foo=bar"));
    setNow("+3");
    docs.push(await startCmd("field3"));
    setNow("+5");
    docs.push(await endCmd("field3 3m"));
    setNow("+5");
    docs.push(await occurCmd("noMetadata -M foo=data dur=4m"));

    await db.destroy().catch(pass);
    setNow("20:00");
  });

  afterAll(() => {
    restoreNow();
  });

  it("Returns a table for an array of documents", () => {
    const table = tableOutput(docs, { show: Show.Standard });
    expect(table).toMatchInlineSnapshot(`
      "       time  field       state  dur       hid  
       17:45:00[90m+0[39m  [7mfield1[27m                       elfuv
       17:46:00[90m+0[39m  [7memotion[27m     happy   ⟝ 10m⟞   en61v
       10:00:00[90m+0[39m  [7mfield2[27m                       d7j7m
      [90mc[39m17:48:00[90m+0[39m  [7mfield2[27m                       w1fla
       17:51:00[90m+0[39m  [7mfield3[27m      [1mstart[22m            mbukz
       17:56:00[90m+0[39m  [7mfield3[27m      [1mend[22m     ⟞ 3m ⟝   puzt1
       18:01:00[90m+0[39m  [7mnoMetadata[27m          ⟝ 4m⟞         
      "
    `);
  });

  it("Can return a table with a different time metric", () => {
    const table = tableOutput(docs, {
      show: Show.Standard,
      timeMetric: "create",
    });
    expect(table).toMatchInlineSnapshot(`
      "       time  field       state  dur       hid  
      [90mc[39m17:45:00[90m+0[39m  [7mfield1[27m                       elfuv
      [90mc[39m17:46:00[90m+0[39m  [7memotion[27m     happy   ⟝ 10m⟞   en61v
      [90mc[39m17:47:00[90m+0[39m  [7mfield2[27m                       d7j7m
      [90mc[39m17:48:00[90m+0[39m  [7mfield2[27m                       w1fla
      [90mc[39m17:51:00[90m+0[39m  [7mfield3[27m      [1mstart[22m            mbukz
      [90mc[39m17:56:00[90m+0[39m  [7mfield3[27m      [1mend[22m     ⟞ 3m ⟝   puzt1
       [90mc[39mundefined  [7mnoMetadata[27m          ⟝ 4m⟞         
      "
    `);
  });

  it("Returns undefined if show is None", () => {
    const table = tableOutput(docs, { show: Show.None });
    expect(table).toBeUndefined();
  });

  it("Returns [No data] if docs is an empty array", () => {
    const table = tableOutput([], { show: Show.Standard });
    expect(table).toMatchInlineSnapshot(`"[No data]"`);
  });

  it("Can return with extra columns", () => {
    const table = tableOutput(docs, {
      show: Show.Standard,
      columns: ["foo"],
    });
    expect(table).toMatchInlineSnapshot(`
      "       time  field       state  dur       hid    foo   
       17:45:00[90m+0[39m  [7mfield1[27m                       elfuv  "bar" 
       17:46:00[90m+0[39m  [7memotion[27m     happy   ⟝ 10m⟞   en61v  "baz" 
       10:00:00[90m+0[39m  [7mfield2[27m                       d7j7m  "foo" 
      [90mc[39m17:48:00[90m+0[39m  [7mfield2[27m                       w1fla  "bar" 
       17:51:00[90m+0[39m  [7mfield3[27m      [1mstart[22m            mbukz        
       17:56:00[90m+0[39m  [7mfield3[27m      [1mend[22m     ⟞ 3m ⟝   puzt1        
       18:01:00[90m+0[39m  [7mnoMetadata[27m          ⟝ 4m⟞           "data"
      "
    `);
  });

  it("Can return using a format string", () => {
    const table = tableOutput(docs, {
      show: Show.Format,
      formatString: "%field%,%foo%,%dur%",
    });
    expect(table).toMatchInlineSnapshot(`
      "field1,bar,
      emotion,baz,PT10M
      field2,foo,
      field2,bar,
      field3,,
      field3,,PT3M
      noMetadata,data,PT4M"
    `);
  });

  it("Retruns an empty string if docs is an empty array with show.format", () => {
    const table = tableOutput([], {
      show: Show.Format,
      formatString: "%field%",
    });
    expect(table).toBe("");
  });
});
