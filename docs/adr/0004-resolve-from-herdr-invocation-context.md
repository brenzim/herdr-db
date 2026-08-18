# Resolve from herdr's invocation context, never from the process working directory

A plugin pane's process starts in the *plugin's install directory*, not in the Worktree the
user invoked it from — verified in herdr's own plugin command log, where a pane opened from a
repository reports `cwd` as the plugin root. herdr instead supplies `HERDR_PLUGIN_CONTEXT_JSON`,
containing a structured `worktree` object (`repo_key`, `repo_name`, `repo_root`,
`checkout_path`, `is_linked_worktree`). We take the Project from `worktree.repo_root`, falling
back to `focused_pane_cwd` then `workspace_cwd`.

## Consequences

`current_dir()` is the obvious call to reach for here and it is *always* wrong — it would
resolve every Project to the plugin's own directory. This is recorded precisely because the
mistake looks correct. The context variable may be absent, empty, or malformed, so it is parsed
defensively with every field optional and no path that can panic. Because herdr hands us
`repo_root` directly, no shelling out to git is needed to identify the Project.
