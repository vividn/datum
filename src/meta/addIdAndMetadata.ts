import {
  DataOnlyPayload,
  DatumData,
  DatumMetadata,
  DatumPayload,
  EitherIdPayload,
  EitherPayload,
} from "../documentControl/DatumDocument.js";
import { AddCmdArgs } from "../commands/addCmd.js";
import { defaultIdComponents } from "../ids/defaultIdComponents.js";
import { buildIdStructure } from "../ids/buildIdStructure.js";
import { defaults } from "../input/defaults.js";
import { newHumanId } from "./newHumanId.js";
import { assembleId } from "../ids/assembleId.js";
import { IdError } from "../errors.js";
import { toDatumTime } from "../time/datumTime.js";
import { now } from "../time/timeUtils.js";
import { compileField } from "../field/compileField.js";

export function addIdAndMetadata<T>(
  data: DatumData<T>,
  args: Pick<AddCmdArgs, "noMetadata" | "idParts" | "idDelimiter">,
): EitherIdPayload<T> {
  let meta: DatumMetadata | undefined = undefined;
  if (!args.noMetadata) {
    meta = {
      humanId: newHumanId(),
    };

    // these will be overwritten later by addDoc, but useful to have them here
    // for undo and original id building
    meta.createTime = toDatumTime(now());
    meta.modifyTime = toDatumTime(now());
  }

  const payload: EitherPayload<T> =
    meta !== undefined
      ? ({ data, meta } as DatumPayload<T>)
      : ({ ...data } as DataOnlyPayload<T>);

  compileField(payload);

  const { defaultIdParts } = defaultIdComponents({
    data,
    meta,
  });

  // Build idStructure for the main part (without the field partition)
  const mainIdStructure = buildIdStructure({
    idParts: args.idParts ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
  });

  // Store the ID structure in metadata - this will be just the main part without field
  if (meta !== undefined && mainIdStructure.match(/(?<!\\)%/)) {
    meta.idStructure = mainIdStructure;
  }

  // Assemble the ID (assembleId now handles adding the field partition)
  const _id = assembleId({
    payload,
    idStructure: mainIdStructure,
  });

  if (_id === "") {
    throw new IdError("Provided or derived _id is blank");
  }

  const idPayload = { _id, ...payload };
  return idPayload;
}
