import { ArgumentParser } from "argparse";
import { ON_AMBIGUOUS_QUICK_ID } from "../ids/quickId";

export type QuickIdArgs = {
  quickId?: string | string[];
  onAmbiguousQuickId?: (typeof ON_AMBIGUOUS_QUICK_ID)[number];
};

export const quickIdArgs = new ArgumentParser({
  add_help: false,
});
const quickIdGroup = quickIdArgs.add_argument_group({
  title: "QuickId",
  description: "Options for quickly referring to documents",
});
// TODO: change to allow with just interspersed commas, not trailing/leading
quickIdGroup.add_argument("quickId", {
  help: 'Can be all or the first few letters of the _id or the humanId of a document. It must match exactly one document unambiguously. If quickId starts or ends with a comma, it acts as a comma separated array of of values, e.g., ",id1,id2". If quickId is not given, the last document(s) added/updated/gotten will be used. For mutative actions this default of using last document(s) is capped at 15 minutes for safety. To explicitly use the last document(s) regardless of time, use the quickId "_LAST".',
  nargs: "?",
});
quickIdGroup.add_argument("--on-ambiguous", {
  help: "What to do if the quickId is ambiguous. all = act on all that match. first = act on the doc with lowest sorted id. last = act on the doc with the highest sorted id. fail = throw an error.",
  choices: [...ON_AMBIGUOUS_QUICK_ID],
  dest: "onAmbiguousQuickId",
});
