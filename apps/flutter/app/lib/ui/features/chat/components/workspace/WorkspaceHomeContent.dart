// ignore_for_file: file_names

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import '../../../../../l10n/generated/app_localizations.dart';
import '../../../../common/CharacterAvatar.dart';
import '../../../../theme/OperitGlassSurface.dart';
import 'WorkspaceOverviewModels.dart';

class WorkspaceHomeContent extends StatelessWidget {
  /// Creates the workspace home content surface.
  const WorkspaceHomeContent({
    super.key,
    required this.workspacePath,
    required this.workspaceUsage,
    required this.terminalSessionCountListenable,
    required this.browserSessionCountListenable,
    required this.onOpenFolder,
    required this.onAddFolder,
    required this.onCreateWorkspace,
    required this.onChooseExistingWorkspace,
    required this.onUnbindWorkspace,
    required this.onOpenTerminal,
    required this.onOpenTerminalSessions,
    required this.onOpenBrowserSessions,
    required this.onOpenBrowser,
  });

  final String? workspacePath;
  final WorkspaceOverviewUsage workspaceUsage;
  final ValueListenable<int> terminalSessionCountListenable;
  final ValueListenable<int> browserSessionCountListenable;
  final ValueChanged<WorkspaceMountedFolder> onOpenFolder;
  final VoidCallback onAddFolder;
  final VoidCallback onCreateWorkspace;
  final VoidCallback onChooseExistingWorkspace;
  final VoidCallback onUnbindWorkspace;
  final VoidCallback onOpenTerminal;
  final VoidCallback onOpenTerminalSessions;
  final VoidCallback onOpenBrowserSessions;
  final VoidCallback onOpenBrowser;

  /// Builds the workspace home tab with overview and common actions.
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final boundWorkspacePath = workspacePath?.trim();
    final boundWorkspaceName = workspaceUsage.workspaceName?.trim();
    final hasBoundWorkspace =
        (boundWorkspacePath != null && boundWorkspacePath.isNotEmpty) ||
        (boundWorkspaceName != null && boundWorkspaceName.isNotEmpty);
    return ListView(
      padding: const EdgeInsets.fromLTRB(14, 14, 14, 14),
      children: <Widget>[
        _WorkspaceStatusSummary(
          workspacePath: boundWorkspacePath,
          workspaceUsage: workspaceUsage,
          terminalSessionCountListenable: terminalSessionCountListenable,
          browserSessionCountListenable: browserSessionCountListenable,
          onOpenFolder: onOpenFolder,
          onAddFolder: onAddFolder,
          onOpenTerminalSessions: onOpenTerminalSessions,
          onOpenBrowserSessions: onOpenBrowserSessions,
        ),
        const SizedBox(height: 8),
        if (hasBoundWorkspace)
          _WorkspacePrimaryAction(
            icon: Icons.link_off,
            title: l10n.workspaceUnbindTitle,
            subtitle: l10n.workspaceUnbindDescription,
            onTap: onUnbindWorkspace,
          ),
        if (!hasBoundWorkspace) ...[
          _WorkspacePrimaryAction(
            icon: Icons.create_new_folder,
            title: l10n.workspaceCreateTitle,
            subtitle: l10n.workspaceCreateDescription,
            onTap: onCreateWorkspace,
          ),
          const SizedBox(height: 8),
          _WorkspacePrimaryAction(
            icon: Icons.folder_open,
            title: l10n.workspaceBindExistingTitle,
            subtitle: l10n.workspaceBindExistingDescription,
            onTap: onChooseExistingWorkspace,
          ),
        ],
        const SizedBox(height: 8),
        _WorkspacePrimaryAction(
          icon: Icons.play_arrow,
          title: l10n.openTerminal,
          subtitle: l10n.openTerminalDescription,
          onTap: onOpenTerminal,
        ),
        const SizedBox(height: 8),
        _WorkspacePrimaryAction(
          icon: Icons.public,
          title: l10n.openBrowser,
          subtitle: l10n.openBrowserDescription,
          onTap: onOpenBrowser,
        ),
      ],
    );
  }
}

class _WorkspaceStatusSummary extends StatelessWidget {
  /// Creates the workspace status summary card.
  const _WorkspaceStatusSummary({
    required this.workspacePath,
    required this.workspaceUsage,
    required this.terminalSessionCountListenable,
    required this.browserSessionCountListenable,
    required this.onOpenFolder,
    required this.onAddFolder,
    required this.onOpenTerminalSessions,
    required this.onOpenBrowserSessions,
  });

  final String? workspacePath;
  final WorkspaceOverviewUsage workspaceUsage;
  final ValueListenable<int> terminalSessionCountListenable;
  final ValueListenable<int> browserSessionCountListenable;
  final ValueChanged<WorkspaceMountedFolder> onOpenFolder;
  final VoidCallback onAddFolder;
  final VoidCallback onOpenTerminalSessions;
  final VoidCallback onOpenBrowserSessions;

  /// Builds the workspace overview card and role usage stack.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final normalizedPath = workspacePath?.trim();
    final workspaceName = workspaceUsage.workspaceName?.trim();
    final hasWorkspace =
        normalizedPath != null && normalizedPath.isNotEmpty ||
        workspaceName != null && workspaceName.isNotEmpty;
    final title = workspaceName != null && workspaceName.isNotEmpty
        ? workspaceName
        : '工作区总览';
    final subtitle = hasWorkspace ? '工作区总览' : '当前对话未绑定工作区';
    return OperitGlassSurface(
      color: Colors.transparent,
      transparentAlpha: 0,
      layer: OperitGlassSurfaceLayer.card,
      borderRadius: BorderRadius.circular(8),
      border: Border.all(
        color: colorScheme.outlineVariant.withValues(alpha: 0.34),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(14, 13, 14, 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Row(
              children: <Widget>[
                Container(
                  width: 46,
                  height: 46,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: colorScheme.surfaceContainerHighest.withValues(
                      alpha: 0.52,
                    ),
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(
                      color: colorScheme.outlineVariant.withValues(alpha: 0.28),
                    ),
                  ),
                  child: Icon(
                    Icons.work_outline,
                    size: 25,
                    color: colorScheme.primary,
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: <Widget>[
                      Text(
                        title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.titleSmall?.copyWith(
                          color: colorScheme.onSurface,
                          fontWeight: FontWeight.w800,
                        ),
                      ),
                      const SizedBox(height: 2),
                      if (hasWorkspace)
                        _WorkspaceCharacterAvatarStrip(
                          usages: workspaceUsage.characterUsages,
                        )
                      else
                        Text(
                          subtitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: colorScheme.onSurfaceVariant,
                          ),
                        ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            ValueListenableBuilder<int>(
              valueListenable: terminalSessionCountListenable,
              builder: (context, terminalSessionCount, child) {
                return ValueListenableBuilder<int>(
                  valueListenable: browserSessionCountListenable,
                  builder: (context, browserSessionCount, child) {
                    return Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: <Widget>[
                        Text(
                          _workspaceSummaryText(workspaceUsage),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: colorScheme.onSurfaceVariant,
                          ),
                        ),
                        const SizedBox(height: 6),
                        Wrap(
                          spacing: 8,
                          runSpacing: 5,
                          children: <Widget>[
                            _WorkspaceSessionButton(
                              icon: Icons.terminal,
                              label: '$terminalSessionCount 个终端',
                              onTap: onOpenTerminalSessions,
                            ),
                            _WorkspaceSessionButton(
                              icon: Icons.public,
                              label: '$browserSessionCount 个浏览器',
                              onTap: onOpenBrowserSessions,
                            ),
                          ],
                        ),
                      ],
                    );
                  },
                );
              },
            ),
            if (hasWorkspace) ...<Widget>[
              const SizedBox(height: 14),
              Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      '挂载文件夹',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.labelLarge?.copyWith(
                        color: colorScheme.onSurface,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),
                  SizedBox.square(
                    dimension: 28,
                    child: IconButton(
                      tooltip: '挂载新文件夹',
                      padding: EdgeInsets.zero,
                      visualDensity: VisualDensity.compact,
                      onPressed: onAddFolder,
                      icon: const Icon(Icons.add, size: 18),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              _WorkspaceMountedFolderStack(
                folders: workspaceUsage.mountedFolders,
                loading: workspaceUsage.mountedFoldersLoading,
                errorMessage: workspaceUsage.mountedFoldersError,
                onOpenFolder: onOpenFolder,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _WorkspaceCharacterAvatarStrip extends StatelessWidget {
  /// Creates the compact character avatar strip under the workspace name.
  const _WorkspaceCharacterAvatarStrip({required this.usages});

  final List<WorkspaceCharacterUsage> usages;

  /// Builds one avatar per character card used in this workspace.
  @override
  Widget build(BuildContext context) {
    if (usages.isEmpty) {
      return const SizedBox(height: 2);
    }
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Wrap(
        spacing: 4,
        runSpacing: 4,
        children: <Widget>[
          for (final usage in usages) _WorkspaceCharacterAvatar(usage: usage),
        ],
      ),
    );
  }
}

class _WorkspaceCharacterAvatar extends StatelessWidget {
  /// Creates one character avatar in the workspace title area.
  const _WorkspaceCharacterAvatar({required this.usage});

  final WorkspaceCharacterUsage usage;

  /// Builds the character avatar with the shared avatar image renderer.
  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    return Tooltip(
      message: '${usage.name} · ${usage.conversationCount} 次',
      waitDuration: const Duration(milliseconds: 450),
      child: Container(
        width: 22,
        height: 22,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: colorScheme.primaryContainer,
          border: Border.all(
            color: colorScheme.surface.withValues(alpha: 0.90),
            width: 1.4,
          ),
        ),
        child: ClipOval(
          child: CharacterAvatarImage(
            avatarUri: usage.avatarUri,
            fit: BoxFit.cover,
          ),
        ),
      ),
    );
  }
}

class _WorkspaceSessionButton extends StatelessWidget {
  /// Creates an explicit session list button.
  const _WorkspaceSessionButton({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final VoidCallback onTap;

  /// Builds a compact text button for session management.
  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    return TextButton.icon(
      onPressed: onTap,
      style: TextButton.styleFrom(
        visualDensity: VisualDensity.compact,
        padding: const EdgeInsetsDirectional.fromSTEB(8, 4, 10, 4),
        minimumSize: Size.zero,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        foregroundColor: colorScheme.primary,
      ),
      icon: Icon(icon, size: 15),
      label: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis),
    );
  }
}

/// Builds the static summary line for workspace overview counts.
String _workspaceSummaryText(WorkspaceOverviewUsage usage) {
  return '${usage.conversationCount} 个对话 · '
      '${usage.characterUsages.length} 个角色卡 · '
      '${usage.mountedFolders.length} 个文件夹';
}

class _WorkspaceMountedFolderStack extends StatelessWidget {
  /// Creates the visible mounted folder list for the workspace overview.
  const _WorkspaceMountedFolderStack({
    required this.folders,
    required this.loading,
    required this.errorMessage,
    required this.onOpenFolder,
  });

  final List<WorkspaceMountedFolder> folders;
  final bool loading;
  final String? errorMessage;
  final ValueChanged<WorkspaceMountedFolder> onOpenFolder;

  /// Builds mounted folder rows with complete path text.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final errorText = errorMessage?.trim();
    if (loading) {
      return _WorkspaceInlineState(
        icon: Icons.sync,
        text: '正在读取挂载文件夹',
        progress: true,
        color: theme.colorScheme.onSurfaceVariant,
      );
    }
    if (errorText != null && errorText.isNotEmpty) {
      return _WorkspaceInlineState(
        icon: Icons.error_outline,
        text: errorText,
        color: theme.colorScheme.error,
      );
    }
    if (folders.isEmpty) {
      return _WorkspaceInlineState(
        icon: Icons.folder_off_outlined,
        text: '当前工作区暂无挂载文件夹',
        color: theme.colorScheme.onSurfaceVariant,
      );
    }

    return Column(
      children: List<Widget>.generate(folders.length, (index) {
        final folder = folders[index];
        return Padding(
          padding: EdgeInsets.only(bottom: index == folders.length - 1 ? 0 : 7),
          child: _WorkspaceMountedFolderTile(
            folder: folder,
            onOpenFolder: onOpenFolder,
          ),
        );
      }),
    );
  }
}

class _WorkspaceMountedFolderTile extends StatelessWidget {
  /// Creates one mounted folder row for the workspace overview.
  const _WorkspaceMountedFolderTile({
    required this.folder,
    required this.onOpenFolder,
  });

  final WorkspaceMountedFolder folder;
  final ValueChanged<WorkspaceMountedFolder> onOpenFolder;

  /// Builds one mounted folder row with full path wrapping.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final path = folder.path.trim();
    return Material(
      color: colorScheme.surfaceContainerHighest.withValues(alpha: 0.22),
      borderRadius: BorderRadius.circular(8),
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: () => onOpenFolder(folder),
        child: DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(8),
            border: Border.all(
              color: colorScheme.outlineVariant.withValues(alpha: 0.24),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 8),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                Container(
                  width: 26,
                  height: 26,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: colorScheme.surfaceContainerLowest.withValues(
                      alpha: 0.66,
                    ),
                    borderRadius: BorderRadius.circular(6),
                    border: Border.all(
                      color: colorScheme.outlineVariant.withValues(alpha: 0.30),
                    ),
                  ),
                  child: Icon(
                    Icons.folder_open_outlined,
                    size: 15,
                    color: colorScheme.primary,
                  ),
                ),
                const SizedBox(width: 9),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: <Widget>[
                      Text(
                        folder.name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: colorScheme.onSurface,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      const SizedBox(height: 3),
                      Tooltip(
                        message: path,
                        waitDuration: const Duration(milliseconds: 450),
                        child: Text(
                          _pathWithBreakOpportunities(path),
                          softWrap: true,
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: colorScheme.onSurfaceVariant,
                            height: 1.22,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                Icon(
                  Icons.chevron_right,
                  size: 17,
                  color: colorScheme.onSurfaceVariant,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _WorkspaceInlineState extends StatelessWidget {
  /// Creates a compact inline state row for overview subsections.
  const _WorkspaceInlineState({
    required this.icon,
    required this.text,
    required this.color,
    this.progress = false,
  });

  final IconData icon;
  final String text;
  final Color color;
  final bool progress;

  /// Builds one inline status row.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: <Widget>[
        if (progress)
          SizedBox.square(
            dimension: 14,
            child: CircularProgressIndicator(strokeWidth: 1.8, color: color),
          )
        else
          Icon(icon, size: 15, color: color),
        const SizedBox(width: 7),
        Expanded(
          child: Text(
            text,
            maxLines: 3,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall?.copyWith(color: color),
          ),
        ),
      ],
    );
  }
}

class _WorkspacePrimaryAction extends StatelessWidget {
  /// Creates one primary workspace action row.
  const _WorkspacePrimaryAction({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;

  /// Builds one primary action row for the workspace home tab.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return OperitGlassSurface(
      color: Colors.transparent,
      transparentAlpha: 0,
      layer: OperitGlassSurfaceLayer.card,
      borderRadius: BorderRadius.circular(8),
      border: Border.all(
        color: theme.colorScheme.outlineVariant.withValues(alpha: 0.30),
      ),
      material: true,
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 10),
          child: Row(
            children: <Widget>[
              Container(
                width: 32,
                height: 32,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: theme.colorScheme.surfaceContainerHighest.withValues(
                    alpha: 0.46,
                  ),
                  borderRadius: BorderRadius.circular(7),
                  border: Border.all(
                    color: theme.colorScheme.outlineVariant.withValues(
                      alpha: 0.26,
                    ),
                  ),
                ),
                child: Icon(icon, size: 18, color: theme.colorScheme.primary),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: <Widget>[
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurface,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      subtitle,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              Icon(
                Icons.chevron_right,
                size: 18,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Adds line-break opportunities after path separators for narrow panels.
String _pathWithBreakOpportunities(String path) {
  return path.replaceAll('/', '/\u200B').replaceAll('\\', '\\\u200B');
}
