import { sum } from '../src';

describe('blah', () => {
  it('works', () => {
    expect(sum(1, 1)).toEqual(2);
  });
});

const arg_test_cases = {
  '--abc 123': {abc: 123},
  'abc=123': {abc: 123},
  '-K a b c=3 -k 1 2': {a: 1, b:2, c: 3},
}
console.log(arg_test_cases)
