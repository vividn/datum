import Nano from "nano";

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};

export const testNano = Nano(`http://admin:password@localhost:5983`);
