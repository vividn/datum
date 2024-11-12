import { endCmd } from "../commands/endCmd";
import { occurCmd } from "../commands/occurCmd";
import { startCmd } from "../commands/startCmd";
import { switchCmd } from "../commands/switchCmd";
import { popNow, pushNow, setNow } from "./test-utils";

export async function generateSampleMorning(date: string): Promise<void> {
  pushNow(`8:30 ${date}`);
  await endCmd("sleep");
  setNow("+5");
  await startCmd("sleep -y -t 2320");
  setNow("8:40");
  await switchCmd("project german");
  await switchCmd("text fiction_book");
  setNow("9:10");
  await endCmd("project");
  await endCmd("text");
  setNow("9:30");
  await switchCmd("environment outside");
  await startCmd("stretch");
  setNow("+7");
  await endCmd("stretch");
  await startCmd("run");
  setNow("+30");
  await endCmd("run distance=5.4");
  await startCmd("stretch");
  setNow("+8");
  await endCmd("stretch");
  await switchCmd("environment home");
  setNow("+3");
  await occurCmd("pushup amount=10");
  setNow("10:30");
  await switchCmd("environment outside 5");
  setNow("11");
  await occurCmd("caffeine amount=100 -c coffee");
  popNow();
}
