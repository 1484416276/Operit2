"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.default = Screen;
const goal_mode_ipc_js_1 = require("../../shared/goal_mode_ipc.js");
const refreshEpochs = new Map();
const refreshPendingEpochs = new Map();
const refreshTimers = new Map();
/** Calculates active goal time from its persisted timing checkpoints. */
function calculateGoalElapsedMs(goal, now) {
    const end = goal.pausedAt ?? goal.completedAt ?? now;
    return Math.max(0, end - goal.startedAt - goal.pausedDurationMs);
}
/** Formats an elapsed duration as a fixed-width hours, minutes, and seconds value. */
function formatGoalDuration(durationMs) {
    const totalSeconds = Math.floor(durationMs / 1000);
    const hours = Math.floor(totalSeconds / 3600).toString().padStart(2, "0");
    const minutes = Math.floor((totalSeconds % 3600) / 60).toString().padStart(2, "0");
    const seconds = (totalSeconds % 60).toString().padStart(2, "0");
    return `${hours}:${minutes}:${seconds}`;
}
/** Renders the current goal and its state controls above the chat composer. */
function Screen(ctx) {
    const [chatId] = ctx.useState("chatId", "");
    const [goal, setGoal] = ctx.useState("goal", null);
    if (!refreshEpochs.has(chatId)) {
        refreshEpochs.set(chatId, 0);
    }
    /** Stops the chat-scoped refresh and invalidates reads started before this stop. */
    const stopRefreshTimer = () => {
        const epoch = (refreshEpochs.get(chatId) ?? 0) + 1;
        refreshEpochs.set(chatId, epoch);
        const timer = refreshTimers.get(chatId);
        if (timer) {
            clearInterval(timer.intervalId);
            refreshTimers.delete(chatId);
        }
        refreshPendingEpochs.delete(chatId);
    };
    /** Starts one shared elapsed-time refresh for the chat. */
    const ensureRefreshTimer = () => {
        if (refreshTimers.has(chatId)) {
            return;
        }
        const epoch = refreshEpochs.get(chatId);
        const timer = { intervalId: 0, epoch };
        timer.intervalId = setInterval(() => {
            if (refreshTimers.get(chatId) !== timer || refreshEpochs.get(chatId) !== epoch) {
                return;
            }
            void refreshGoal(epoch);
        }, 1000);
        refreshTimers.set(chatId, timer);
    };
    /** Reads the persisted goal and refreshes the rendered elapsed time. */
    const refreshGoal = async (expectedEpoch = refreshEpochs.get(chatId)) => {
        if (expectedEpoch !== refreshEpochs.get(chatId)) {
            return;
        }
        if (refreshPendingEpochs.get(chatId) === expectedEpoch) {
            return;
        }
        refreshPendingEpochs.set(chatId, expectedEpoch);
        try {
            const nextGoal = await (0, goal_mode_ipc_js_1.readGoalAsync)(chatId);
            if (expectedEpoch !== refreshEpochs.get(chatId)) {
                return;
            }
            if (!nextGoal || nextGoal.status !== "active") {
                stopRefreshTimer();
                setGoal(nextGoal);
                return;
            }
            ensureRefreshTimer();
            setGoal(nextGoal);
        }
        finally {
            if (refreshPendingEpochs.get(chatId) === expectedEpoch) {
                refreshPendingEpochs.delete(chatId);
            }
        }
    };
    /** Refreshes the goal snapshot stored by the owning plugin runtime. */
    const refresh = async () => {
        await refreshGoal();
    };
    /** Pauses an active goal. */
    const pause = async () => {
        stopRefreshTimer();
        setGoal(await (0, goal_mode_ipc_js_1.setGoalStatusAsync)(chatId, "paused"));
    };
    /** Resumes a paused goal. */
    const resume = async () => {
        stopRefreshTimer();
        setGoal(await (0, goal_mode_ipc_js_1.setGoalStatusAsync)(chatId, "active"));
        ensureRefreshTimer();
    };
    /** Removes the current goal. */
    const clear = async () => {
        stopRefreshTimer();
        await (0, goal_mode_ipc_js_1.clearGoalAsync)(chatId);
        setGoal(null);
    };
    const active = goal?.status === "active";
    // Keep the empty slot zero-height while the initial read is pending too.
    // Transient padding here resizes the transcript and nudges its bottom twice.
    if (goal === null) {
        return ctx.UI.Column({ fillMaxWidth: true, onLoad: refresh }, []);
    }
    const paused = goal?.status === "paused";
    const title = active ? "Task goal" : paused ? "Task goal paused" : "Task goal completed";
    const elapsed = goal ? formatGoalDuration(calculateGoalElapsedMs(goal, Date.now())) : "00:00:00";
    return ctx.UI.Column({ fillMaxWidth: true, padding: { horizontal: 8, vertical: 2 }, onLoad: refresh }, [
        ctx.UI.Card({
            fillMaxWidth: true,
            containerColor: ctx.MaterialTheme.colorScheme.surfaceVariant.copy({ alpha: 0.22 }),
            shape: { cornerRadius: 6 },
            elevation: 0,
        }, [
            ctx.UI.Row({ fillMaxWidth: true, padding: { horizontal: 8, vertical: 3 }, spacing: 6, verticalAlignment: "center" }, [
                ctx.UI.Box({ width: 32, height: 32, contentAlignment: "center" }, ctx.UI.Icon({ name: "flag", tint: "onSurfaceVariant", size: 16 })),
                ctx.UI.Text({ text: title, style: "labelMedium", maxLines: 1, overflow: "ellipsis", weight: 1 }),
                ctx.UI.Text({ text: elapsed, style: "labelSmall", color: "onSurfaceVariant", maxLines: 1 }),
                ctx.UI.Text({ text: goal.objective, style: "bodySmall", fontSize: 12, maxLines: 1, overflow: "ellipsis", weight: 2 }),
                active
                    ? ctx.UI.IconButton({ icon: "pause", width: 32, height: 32, onClick: pause })
                    : paused
                        ? ctx.UI.IconButton({ icon: "playArrow", width: 32, height: 32, onClick: resume })
                        : ctx.UI.Box({ width: 32, height: 32, contentAlignment: "center" }, ctx.UI.Icon({ name: "checkCircle", tint: "onSurfaceVariant", size: 16 })),
                ctx.UI.IconButton({
                    width: 32,
                    height: 32,
                    onClick: clear,
                    content: ctx.UI.Icon({ name: "delete", tint: "onSurfaceVariant", size: 16 }),
                }),
            ]),
        ]),
    ]);
}
