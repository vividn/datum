import { _emit } from "../emit";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { JsonObject } from "../../utils/utilityTypes";

type DocType = EitherDocument;
type MapKey = string[][];
type MapValue = null;
type ReduceValue = number;

function emit(_key: MapKey, _value: MapValue): void {
  _emit(_key, _value);
}

export const structuresView: DatumView<DocType, MapKey, MapValue, ReduceValue> =
  {
    name: "datum_structures",
    map: (doc) => {
      function sortedSubkeys(obj: JsonObject): string[][] {
        const subkeysByDepth: string[][] = [];
        const topOrderKeys: string[] = [];
        const keys = Object.keys(obj).sort();
        for (const key of keys) {
          topOrderKeys.push(key);
          const val = obj[key];
          if (typeof val === "object" && val !== null && !Array.isArray(val)) {
            sortedSubkeys(val).map((subkeyArray, index) => {
              const prefixedSubKeys = subkeyArray.map(
                (subkey) => key + "." + subkey,
              );
              (subkeysByDepth[index] = subkeysByDepth[index] || []).push(
                ...prefixedSubKeys,
              );
            });
          }
        }
        return [topOrderKeys, ...subkeysByDepth];
      }

      delete (doc as any)["_rev"];
      delete (doc as any)["_id"];
      const structure = sortedSubkeys(doc);

      emit(structure, null);
    },
    reduce: "_count",
  };
