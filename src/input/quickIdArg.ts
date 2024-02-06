import { ArgumentParser } from "argparse";

export type QuickIdArg = {
  quickId: string | string[];
};

export function quickIdArgs(parser: ArgumentParser): ArgumentParser {
  const quickIdGroup = parser.add_argument_group({
    title: "QuickId",
    description: "Options for quickly referring to documents",
  });
  quickIdGroup.add_argument("quickId", {
    help:
      "Can be all or the first few letters of the _id or " +
      "the humanId of a document. It must match exactly one document " +
      "unambiguously. If quickId starts or ends with a comma, it" +
      'acts as a comma separated array of of values, e.g., ",id1,id2"',
  });
  return parser;
}
