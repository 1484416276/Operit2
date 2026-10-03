import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";

/** Loads the shipped panel with a deferred goal read and observable UI state. */
async function panelHarness() {
  const source = await readFile(new URL(
    "../packages/buildin/goal_mode/dist/ui/goal_panel/index.ui.js", import.meta.url,
  ), "utf8");
  let resolveRead;
  const read = new Promise(resolve => { resolveRead = resolve; });
  const exports = {};
  let reads = 0;
  const timers = new Map();
  let nextTimerId = 0;
  runInNewContext(source, {
    exports,
    require(name) {
      assert.equal(name, "../../shared/goal_mode_ipc.js");
      return {
        readGoalAsync() { reads++; return read; },
        clearGoalAsync: async () => {},
      };
    },
    setInterval(callback) {
      const id = ++nextTimerId;
      timers.set(id, callback);
      return id;
    },
    clearInterval(id) { timers.delete(id); },
  });
  const state = new Map([["chatId", "layout-test"]]);
  const ctx = {
    useState(key, initial) {
      if (!state.has(key)) state.set(key, initial);
      return [state.get(key), value => state.set(key, value)];
    },
    UI: new Proxy({}, {
      get: (_, type) => (props, children = []) => ({ type, props, children }),
    }),
    MaterialTheme: { colorScheme: { surfaceVariant: { copy: value => value } } },
  };
  return {
    render: () => exports.default(ctx),
    resolveRead,
    timers,
    get reads() { return reads; },
  };
}

/** An empty composer contribution must not shrink the transcript, even briefly. */
function expectZeroHeightPanel(node) {
  assert.equal(node.type, "Column");
  assert.equal(node.children.length, 0);
  assert.equal(node.props.padding, undefined);
  assert.equal(node.props.height, undefined);
  assert.equal(typeof node.props.onLoad, "function", "Empty panels must still load their goal");
}

function descendants(node) {
  return [node, ...[node.children].flat().filter(Boolean).flatMap(descendants)];
}

test("empty goal panel reserves no space before, during, or after its async read", async () => {
  const panel = await panelHarness();
  const initial = panel.render();
  expectZeroHeightPanel(initial);
  const loaded = initial.props.onLoad();
  assert.equal(panel.reads, 1);
  expectZeroHeightPanel(panel.render());
  panel.resolveRead(null);
  await loaded;
  expectZeroHeightPanel(panel.render());
  assert.equal(panel.timers.size, 0);
});

for (const status of ["active", "paused", "completed"]) {
  test(`loaded ${status} goal retains its card and becomes zero-height when cleared`, async () => {
    const panel = await panelHarness();
    const initial = panel.render();
    expectZeroHeightPanel(initial);
    const loaded = initial.props.onLoad();
    panel.resolveRead({
      objective: "Keep the goal card visible",
      status,
      startedAt: 1000,
      pausedAt: status === "paused" ? 2000 : null,
      completedAt: status === "completed" ? 2000 : null,
      pausedDurationMs: 0,
    });
    await loaded;
    const card = panel.render();
    assert.equal(card.props.padding.vertical, 2);
    assert.equal(card.children[0].type, "Card");
    assert.ok(descendants(card).some(node => node.props.text === "Keep the goal card visible"));
    assert.equal(panel.timers.size, status === "active" ? 1 : 0);
    const remove = descendants(card).find(node => node.props.content?.props.name === "delete");
    assert.ok(remove);
    await remove.props.onClick();
    expectZeroHeightPanel(panel.render());
    assert.equal(panel.timers.size, 0);
  });
}
