# ToolPkg XML render registry refresh

Existing chat XML must be resolved again when plugin hooks become available, even
when the persisted message content has not changed. This preserves the behavior
of the original Compose application's observable XML registry version.

## Notification boundary

`ToolPkgXmlRenderBridge` owns the committed hook list and a `StateFlow<i64>`
revision. Synchronization normalizes and sorts registrations, compares the full
hook declarations, and commits changed hooks before publishing a new revision.
Publication occurs after releasing the hook mutex, so observers can request the
new renderer without reentering that lock. Identical registrations do not publish
another revision. Hook additions, changed declarations, and removals do.

`ChatServiceCore.xmlRenderRegistryRevisionFlow()` exposes this flow through the
existing Core proxy watch protocol. It is independent of installation catalog
notifications and the plugin loading overlay: those events do not establish that
XML registrations have been committed.

## Flutter lifecycle

Each mounted ToolPkg XML bridge watches the selected chat runtime's revision,
including nodes for which no hook currently exists. The initial snapshot also
invalidates output, covering registrations committed between the first render
request and subscription establishment. Duplicate revisions are ignored.

A revision creates a fresh render future and changes the FutureBuilder generation
key, invalidating old results and embedded DSL hosts. XML text updates within the
same registry generation retain the existing host while the next result is
pending. Responses from an older generation cannot replace the current output.
The subscription is detached when the runtime changes or the XML node is disposed.

No message data is modified and the chat page does not need to be reopened.
All notifications use the existing platform-neutral StateFlow/Core proxy layers.

## Checks

- `apps/flutter/app/test/xml_render_registry_refresh_test.dart`: an already
  mounted XML node reacts to registration/removal, repeated revisions cause no
  extra request, obsolete responses are ignored, and runtime subscriptions detach.
- `ToolPkgXmlRenderBridge.rs` unit tests: only changed hook declarations increment
  revisions, and observers can read committed hooks after their lock is released.
- Flutter commands run via FVM from `apps/flutter/app`.
