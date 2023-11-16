import { restoreCmd } from "../restoreCmd";

describe("restoreCmd", () => {
  it("works", async () => {
    await restoreCmd({
      filename: "backup.json.br",
    });
  });
  it.todo("creates a new db if it does not exist");
  it.todo("throws an error if the db is not empty");
  it.todo("can restore over a nonempty db with the appropriate option");
});
