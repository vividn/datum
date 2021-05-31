// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};
