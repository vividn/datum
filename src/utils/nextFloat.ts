// From: https://stackoverflow.com/a/72185420/8242329
// Author: T.J. Crowder
// This function is released under the Creative Commons licensing terms [(CC BY-SA 4.0)](https://creativecommons.org/licenses/by-sa/4.0/) as per StackOverflow's Terms of Service

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
