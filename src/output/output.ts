import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import chalk from "chalk";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import {
  ACTIONS,
  actionId,
  extractFormatted,
  ExtractedAndFormatted,
  formattedDoc,
  formattedNonRedundantData,
} from "./format";

function headerLine(formatted: ExtractedAndFormatted): string {
  return [formatted.action, formatted.hid, formatted.id]
    .filter(Boolean)
    .join(" ");
}

function mainInfoLine(formatted: ExtractedAndFormatted): string | undefined {
  const infoLine = [
    formatted.time.occur,
    formatted.field,
    formatted.state,
    formatted.dur,
  ]
    .filter(Boolean)
    .join(" ");
  return infoLine !== "" ? infoLine : undefined;
}

export function customFormat(
  payload: EitherPayload,
  formatString: string,
): string {
  const { data, meta } = pullOutData(payload);
  return interpolateFields({ data, meta, format: formatString });
}

export function showRename(
  beforeId: string,
  afterId: string,
  outputArgs: OutputArgs,
): string | undefined {
  const { show } = sanitizeOutputArgs(outputArgs);
  if (show === Show.None || show === Show.Format) {
    return undefined;
  }
  const output =
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId);
  console.log(output);
  return output;
}

export function showSingle(
  action: ACTIONS,
  doc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  const { show, formatString } = sanitizeOutputArgs(outputArgs);
  const extracted = extractFormatted(doc, action);

  if (show === Show.None) {
    return undefined;
  }

  if (show === Show.Format) {
    if (formatString === undefined) {
      throw new Error(
        "MissingArgument: formatted show requested without a format string",
      );
    }
    const output = customFormat(doc, formatString);
    console.log(output);
    return output;
  }

  const outputs: string[] = [];

  if (show === Show.Minimal) {
    if (action !== ACTIONS.NoDiff) {
      const header = headerLine(extracted);
      console.log(header);
      outputs.push(header);
    }
    return outputs.length > 0 ? outputs.join("\n") : undefined;
  }
  const header = headerLine(extracted);
  console.log(header);
  outputs.push(header);
  const mainInfo = mainInfoLine(extracted);
  if (mainInfo) {
    console.log(mainInfo);
    outputs.push(mainInfo);
  }

  if (formatString) {
    const formatted = customFormat(doc, formatString);
    console.log(formatted);
    outputs.push(formatted);
  }

  if (show === Show.All) {
    const formatted = formattedDoc(doc);
    console.log(formatted);
    outputs.push(formatted);
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    const formattedData = formattedNonRedundantData(doc);
    if (formattedData !== undefined) {
      console.log(formattedData);
      outputs.push(formattedData);
    }
  }

  return outputs.length > 0 ? outputs.join("\n") : undefined;
}
export function showCreate(
  doc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(ACTIONS.Create, doc, outputArgs);
}

export function showExists(
  doc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(ACTIONS.Exists, doc, outputArgs);
}
export function showNoDiff(
  doc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(ACTIONS.NoDiff, doc, outputArgs);
}
export function showFailed(
  payload: EitherPayload,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(
    ACTIONS.Failed,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs,
  );
}
export function showDelete(
  payload: EitherPayload,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(
    ACTIONS.Delete,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs,
  );
}

export function showUpdate(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(ACTIONS.Update, afterDoc, outputArgs);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs,
): string | undefined {
  return showSingle(ACTIONS.OWrite, afterDoc, outputArgs);
}

function sanitizeOutputArgs(outputArgs: OutputArgs): {
  show: Show;
  formatString?: string;
} {
  const show =
    outputArgs.show === Show.None
      ? Show.None
      : ((outputArgs.showAll ? Show.All : outputArgs.show) ??
        (outputArgs.formatString ? Show.Format : Show.None));
  return { show, formatString: outputArgs.formatString };
}
