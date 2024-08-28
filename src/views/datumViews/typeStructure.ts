import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = string[];
type MapValue = null;
type ReduceValue = number;

function emit(_key: MapKey, _value: MapValue): void {
  _emit(_key, _value);
}

export const typeStructureView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue
> = {
  name: "datum_type_structure",
  emit,
  map: (doc) => {},
  reduce: "_count",
};
