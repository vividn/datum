declare module 'string.ify' {
    export default function stringify(
      value: any,
      options?: {
        maxDepth?: number;
        maxLength?: number;
        maxArrayLength?: number;
        maxObjectLength?: number;
        maxStringLength?: number;
        indent?: string | number;
        skipFields?: string[];
      }
    ): string;
  }