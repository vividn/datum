import {
  DataOnlyPayload,
  DatumData,
  DatumMetadata,
  DatumPayload,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { AddCmdArgs } from "../commands/addCmd";
import { defaultIdComponents } from "../ids/defaultIdComponents";
import { buildIdStructure } from "../ids/buildIdStructure";
import { defaults } from "../input/defaults";
import { newHumanId } from "./newHumanId";
import { DateTime } from "luxon";
import { assembleId } from "../ids/assembleId";
import { IdError } from "../errors";

export function addIdAndMetadata<T>(
  data: DatumData<T>,
  args: Pick<AddCmdArgs, "noMetadata" | "idPart" | "idDelimiter" | "partition">
): EitherPayload<T> {
  const { defaultIdParts, defaultPartitionParts } = defaultIdComponents({
    data,
  });

  const idStructure = buildIdStructure({
    idParts: args.idPart ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
    partition: args.partition ?? defaultPartitionParts,
  });

  let meta: DatumMetadata | undefined = undefined;
  if (!args.noMetadata) {
    meta = {
      humanId: newHumanId(),
      random: Math.random(),
    };

    // these will be overwritten later by addDoc, but useful to have them here
    // for undo and original id building
    const now = DateTime.utc().toString();
    meta.createTime = now;
    meta.modifyTime = now;

    // don't include idStructure if it is just a raw string (i.e. has no field references in it)
    // that would be a waste of bits since _id then is exactly the same
    if (idStructure.match(/(?<!\\)%/)) {
      meta.idStructure = idStructure;
    }
  }
  const payload: EitherPayload<T> =
    meta !== undefined
      ? ({ data, meta } as DatumPayload<T>)
      : ({ ...data } as DataOnlyPayload<T>);

  const _id = assembleId({
    payload,
    idStructure,
  });
  if (_id === "") {
    throw new IdError("Provided or derived _id is blank");
  }
  payload._id = _id;
  return payload;
}
