import { EitherDocument, EitherPayload } from "../documentControl/DatumDocument";
import chalk from "chalk";
import { OutputArgs, Show } from "../input/outputArgs";
import { interpolateFields } from "../utils/interpolateFields";
import { pullOutData } from "../utils/pullOutData";
import {
  ACTIONS,
  ACTION_CHALK,
  actionId,
  extractFormatted,
  ExtractedAndFormatted,
  formattedDoc,
  formattedNonRedundantData,
} from "./format";


function showHeaderLine(formatted: ExtractedAndFormatted): void {
  console.log(
    [formatted.action, formatted.hid, formatted.id].filter(Boolean).join(" "),
  );
}

function showMainInfoLine(formatted: ExtractedAndFormatted): void {
  const infoLine = [
    formatted.time.occur,
    formatted.field,
    formatted.state,
    formatted.dur,
  ]
    .filter(Boolean)
    .join(" ");
  if (infoLine !== "") {
    console.log(infoLine);
  }
}

export function showCustomFormat(
  payload: EitherPayload,
  formatString: string,
): void {
  const { data, meta } = pullOutData(payload);
  const outputString = interpolateFields({ data, meta, format: formatString });
  console.log(outputString);
}

export function showRename(
  beforeId: string,
  afterId: string,
  outputArgs: OutputArgs,
): void {
  const { show } = sanitizeOutputArgs(outputArgs);
  if (show === Show.None || show === Show.Format) {
    return;
  }
  console.log(
    actionId(ACTIONS.Rename, beforeId) + " ⟶ " + chalk.green(afterId),
  );
}

export function showSingle(
  action: ACTIONS,
  doc: EitherDocument,
  outputArgs: OutputArgs,
): void {
  const { show, formatString } = sanitizeOutputArgs(outputArgs);
  const extracted = extractFormatted(doc, action);

  if (show === Show.None) {
    return;
  }

  if (show === Show.Format) {
    if (formatString === undefined) {
      throw new Error(
        "MissingArgument: formatted show requested without a format string",
      );
    }
    showCustomFormat(doc, formatString);
    return;
  }

  if (show === Show.Minimal) {
    if (action !== ACTIONS.NoDiff) {
      showHeaderLine(extracted);
    }
    return;
  }
  showHeaderLine(extracted);
  showMainInfoLine(extracted);

  if (formatString) {
    showCustomFormat(doc, formatString);
  }

  if (show === Show.All) {
    console.log(formattedDoc(doc));
  }

  if (
    show === Show.Standard ||
    (show === Show.Default && formatString === undefined)
  ) {
    const formattedData = formattedNonRedundantData(doc);
    if (formattedData !== undefined) {
      console.log(formattedData);
    }
  }
}
export function showCreate(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Create, doc, outputArgs);
}

export function showExists(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.Exists, doc, outputArgs);
}
export function showNoDiff(doc: EitherDocument, outputArgs: OutputArgs): void {
  return showSingle(ACTIONS.NoDiff, doc, outputArgs);
}
export function showFailed(
  payload: EitherPayload,
  outputArgs: OutputArgs,
): void {
  return showSingle(
    ACTIONS.Failed,
    { _id: "", _rev: "", ...payload } as EitherDocument,
    outputArgs,
  );
}
export function showDelete(
  payload: EitherPayload,
  outputArgs: OutputArgs,
): void {
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
): void {
  return showSingle(ACTIONS.Update, afterDoc, outputArgs);
}
export function showOWrite(
  _beforeDoc: EitherDocument,
  afterDoc: EitherDocument,
  outputArgs: OutputArgs,
): void {
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
