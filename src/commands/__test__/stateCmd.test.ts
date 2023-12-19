import { testDbLifecycle } from "../../__test__/test-utils";

describe("stateCmd", () => {
  const dbName = "state_cmd_test";
  testDbLifecycle(dbName);
});
