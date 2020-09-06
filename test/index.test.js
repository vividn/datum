const main = require('../src/index');

describe("main", () => {
    test("main logs hello world", () => {
        const mockLog = jest.fn();
        console.log = mockLog;

        main();

        expect(mockLog).toBeCalledTimes(1);
        expect(mockLog.mock.calls[0][0]).toEqual("Hello World!");
    })
});
