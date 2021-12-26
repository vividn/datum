import yargs, { Argv } from "yargs";

export type QuickIdArg = {
  quickId: string;
};

export function quickIdArg(otherYargs?: Argv): Argv {
  const yarg = otherYargs ?? yargs;
  return yarg.positional("quickId", {
    describe:
      "Can be all or the first few letters of the _id or " +
      "the humanId of a document. It must match exactly one document " +
      "unambiguously.",
    type: "string",
  });
}
