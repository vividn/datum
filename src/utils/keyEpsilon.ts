import { JsonType } from "./utilityTypes";

export const HIGH_STRING = "\uffff\uffff\uffff\uffff" as const;

export function incrementString(str: string): string {
  return str + "\u0001";
}

export function decrementString(str: string): string {
  const lastChar = str.slice(-1);
  if (lastChar === "\u0001" || lastChar === "\u0000") {
    return str.slice(0, -1);
  } else {
    const oneLess = String.fromCharCode(lastChar.charCodeAt(0) - 1);
    return str.slice(0, -1) + oneLess + HIGH_STRING;
  }
}

// From: https://stackoverflow.com/a/72185420/8242329
// A JavaScript implementation of OpenJDK's `Double.nextAfter` method.
export function nextFloat(start: number, direction: number) {
  // These arrays share their underlying memory, letting us use them to do what
  // Java's `Double.doubleToRawLongBits` and `Double.longBitsToDouble` do.
  const f64 = new Float64Array(1);
  const b64 = new BigInt64Array(f64.buffer);

  // Branch to descending case first as it is more costly than ascending
  // case due to start != 0.0d conditional.
  if (start > direction) {
    // descending
    if (start !== 0) {
      f64[0] = start;
      const transducer = b64[0];
      b64[0] = transducer + (transducer > 0n ? -1n : 1n);
      return f64[0];
    } else {
      // start == 0.0d && direction < 0.0d
      return -Number.MIN_VALUE;
    }
  } else if (start < direction) {
    // ascending
    // Add +0.0 to get rid of a -0.0 (+0.0 + -0.0 => +0.0)
    // then bitwise convert start to integer.
    f64[0] = start + 0;
    const transducer = b64[0];
    b64[0] = transducer + (transducer >= 0n ? 1n : -1n);
    return f64[0];
  } else if (start === direction) {
    return direction;
  } else {
    // isNaN(start) || isNaN(direction)
    return start + direction;
  }
}

export function incrementKey(key: JsonType): JsonType {
  if (key === null) {
    return false;
  }
  if (key === false) {
    return true;
  }
  if (key === true) {
    return -Number.MAX_VALUE;
  }
  if (typeof key === "number") {
    if (key === Number.MAX_VALUE) {
      return "";
    }
    return nextFloat(key, +Infinity);
  }
  if (typeof key === "string") {
    return incrementString(key);
  }
  if (Array.isArray(key)) {
    if (key.length === 0) {
      return [null];
    }
    const incrementedLastValue = incrementKey(key.at(-1)!);
    return [...key.slice(0, -1), incrementedLastValue];
  }
  key satisfies Record<string, JsonType>;
  if (Object.keys(key).length === 0) {
    return { "": null };
  }
  const lastKey = Object.keys(key).at(-1)!;
  return { ...key, [lastKey]: incrementKey(key[lastKey]) };
}

export function decrementKey(key: JsonType): JsonType {
  if (key === null) {
    return null;
  }
  if (key === false) {
    return null;
  }
  if (key === true) {
    return false;
  }
  if (typeof key === "number") {
    if (key === -Number.MAX_VALUE) {
      return true;
    }
    if (key === Number.MAX_VALUE) {
      return "";
    }
    return nextFloat(key, -Infinity);
  }
  if (typeof key === "string") {
    return decrementString(key);
  }
  if (Array.isArray(key)) {
    if (key.length === 0) {
      return HIGH_STRING;
    }
    const lastKey = key.at(-1)!;
    if (lastKey === null) {
      return key.slice(0, -1);
    }
    const decrementedLastValue = decrementKey(key.at(-1)!);
    return [...key.slice(0, -1), decrementedLastValue];
  }
  key satisfies Record<string, JsonType>;
  if (Object.keys(key).length === 0) {
    return Array(6).fill(HIGH_STRING);
  }
  const lastKey = Object.keys(key).at(-1)!;
  if (key[lastKey] === null) {
    const clone = { ...key };
    delete clone[lastKey];
    return {
      ...clone,
      [decrementString(lastKey)]: { [HIGH_STRING]: HIGH_STRING },
    };
  }
  return { ...key, [lastKey]: decrementKey(key[lastKey]) };
}
