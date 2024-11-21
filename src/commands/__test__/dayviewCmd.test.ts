import { testDbLifecycle } from "../../__test__/test-utils";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";
import { setupCmd } from "../setupCmd";
import { dayviewCmd } from "../dayviewCmd";
import fs from "fs";

describe("dayview", () => {
  const dbName = "dayview_test";
  testDbLifecycle(dbName);

  beforeEach(async () => {
    setupCmd({});
    generateSampleMorning("2024-11-21");
  });

  it("consistently generates a dayview svg", async () => {
    await dayviewCmd({
      endDate: "2024-11-21",
      nDays: 1,
      width: 1000,
      dayHeight: 100,
      outputFile: "/tmp/dayview.svg",
    });
    const svg = fs.readFileSync("/tmp/dayview.svg", "utf8");
    expect(svg).toMatchSnapshot();
  });
});
