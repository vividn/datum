import { EitherDocument } from "../../documentControl/DatumDocument";
import { JsonObject, JsonType } from "../../utils/utilityTypes";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = string[];
type MapValue = null;
type ReduceValue = number;

function emit(_key: MapKey, _value: MapValue): void {
  _emit(_key, _value);
}

type typeName =
  | "number"
  | "string"
  | "boolean"
  | "object"
  | "null"
  | `{${string}}`
  | `${typeName}|${typeName}`
  | `${typeName}[]`
  | `(${typeName})[]`;

export const typeStructureView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue
> = {
  name: "datum_type_structure",
  emit,
  map: (doc) => {
    function typeOf(v: JsonType): typeName {
      if (Array.isArray(v)) {
        const arrayTypes = v.map((element) => typeOf(element));
        const sortedUnique = [...new Set(arrayTypes)].sort();
        if (sortedUnique.length === 1) {
          return `${sortedUnique[0]}[]`;
        } else {
          return `(${sortedUnique.join("|")})[]`;
        }
      }
      if (v === null) {
        return "null";
      }
      if (typeof v === "object") {
        return typeof v._type === "string" ? `{${v._type}}` : "object";
      }
      if (typeof v === "number") {
        return "number";
      }
      if (typeof v === "string") {
        return "string";
      }
      if (typeof v === "boolean") {
        return "boolean";
      }
      return "unknown";
    }

    function emitSubkeyTypes(obj: JsonObject, parents: string[] = []) {
      for (const key in obj) {
        const value = obj[key];
        const type = typeOf(value);
        emit([...parents, key, `<${type}>`], null);
        if (type === "object" || type.startsWith("{")) {
          emitSubkeyTypes(value as JsonObject, [...parents, key]);
        }
      }
    }

    emitSubkeyTypes(doc);
  },
  reduce: "_count",
};
