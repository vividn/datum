// A temp directory of this test process's own, gone once the process is,
// however it ends (da-bvhac).
//
// Every browser Playwright launches makes `playwright-artifacts-*` and, with
// no profile given, `playwright_<engine>dev_profile-*` under `os.tmpdir()`,
// which it reads at each launch. It removes them on a close and in its exit
// handler, and they were left wherever neither runs: a launch that fails
// once the two are made (an executable missing, as webkit was here for the
// month after Playwright 1.63, every `launchable` probe a pair), and a process
// killed by SIGINT or SIGKILL. So `TMPDIR` is this directory, set before any
// launch, and a reaper outside the process group removes it once the pipe
// from this process closes, which every death does. It changes no signal's
// handling: Playwright's and node's stay as they were.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'datum-test-'));
process.env.TMPDIR = dir;
// Again after a second or two, for a browser orphaned by a SIGKILL that
// writes on until it reads its pipe from this process closed.
const reaper = spawn('sh', ['-c', 'cat >/dev/null; for _ in 1 2 3; do rm -rf "$0"; sleep 1; done', dir], {
	detached: true,
	stdio: ['pipe', 'ignore', 'ignore']
});
// Where there is no `sh` (Windows) nothing is reaped, and nothing breaks.
reaper.on('error', () => {});
reaper.unref();
reaper.stdin.unref();
