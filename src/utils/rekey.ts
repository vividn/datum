import { GenericObject } from "../GenericObject";

export function rekey(aData: GenericObject, bData: GenericObject): GenericObject {
  
  const newData = { ...aData, ...bData };
  return newData;
}
