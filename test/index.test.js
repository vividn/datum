const main = require('../src/index');
const nock = require('nock');
const chai = require('chai');
const chaiNock = require('chai-nock');

chai.use(chaiNock);

const couchNock = nock('http://localhost:5984')
.post('/datum')
.reply(200, (uri, requestBody) => ({
    "ok": true,
    "id": requestBody["_id"] || "nock_generated_id",
    "rev": "nock_generated_revision"
}));

describe("main", () => {
    beforeEach(() => {
        nock.cleanAll();
    });

    test("main logs hello world", () => {
        const mockLog = jest.fn();
        console.log = mockLog;

        main();

        expect(mockLog).toBeCalledTimes(1);
        expect(mockLog.mock.calls[0][0]).toEqual("Hello World!");
    });

    it("makes an entry into couchdb", () => {
        main();

        expect(couchNock.isDone()).toEqual(true);
    });
});
