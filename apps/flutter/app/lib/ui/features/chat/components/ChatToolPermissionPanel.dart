// ignore_for_file: file_names

import 'package:flutter/material.dart';

import '../../../../core/proxy/generated/CoreProxyModels.g.dart' as core_proxy;
import '../../../../l10n/generated/app_localizations.dart';
import '../viewmodel/ChatViewModel.dart';

class ChatToolPermissionPanel extends StatefulWidget {
  /// Creates the inline permission state surface for one chat.
  const ChatToolPermissionPanel({
    super.key,
    required this.request,
    required this.onRespond,
  });

  final core_proxy.RuntimeHostInteractionToolPermissionRequest request;
  final Future<void> Function(
    String chatId,
    String requestId,
    ChatToolPermissionResult result,
  )
  onRespond;

  @override
  State<ChatToolPermissionPanel> createState() =>
      _ChatToolPermissionPanelState();
}

class _ChatToolPermissionPanelState extends State<ChatToolPermissionPanel> {
  bool _submitting = false;
  bool _parametersExpanded = false;

  @override
  void didUpdateWidget(covariant ChatToolPermissionPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.request.requestId != widget.request.requestId ||
        oldWidget.request.chatId != widget.request.chatId) {
      _submitting = false;
      _parametersExpanded = false;
    }
  }

  /// Sends one decision for the request displayed by this chat.
  Future<void> _respond(ChatToolPermissionResult result) async {
    if (_submitting) {
      return;
    }
    setState(() {
      _submitting = true;
    });
    final request = widget.request;
    try {
      await widget.onRespond(request.chatId, request.requestId, result);
    } catch (error, stackTrace) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: error,
          stack: stackTrace,
          library: 'chat tool permission panel',
          context: ErrorDescription('responding to chat tool permission'),
        ),
      );
      if (mounted &&
          widget.request.requestId == request.requestId &&
          widget.request.chatId == request.chatId) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('${AppLocalizations.of(context)!.failed}: $error'),
          ),
        );
        setState(() {
          _submitting = false;
        });
      }
    }
  }

  /// Builds compact actions that keep the one-time approval visually primary.
  Widget _buildActions(BuildContext context, AppLocalizations l10n) {
    final colorScheme = Theme.of(context).colorScheme;
    return LayoutBuilder(
      builder: (context, constraints) {
        final denyAction = Tooltip(
          message: l10n.toolApprovalDeny,
          child: IconButton(
            onPressed: _submitting
                ? null
                : () => _respond(ChatToolPermissionResult.deny),
            visualDensity: VisualDensity.compact,
            constraints: const BoxConstraints.tightFor(width: 36, height: 36),
            iconSize: 18,
            color: colorScheme.onSurfaceVariant,
            icon: const Icon(Icons.close_rounded),
          ),
        );
        final sessionAction = Tooltip(
          message: l10n.toolApprovalAlwaysAllow,
          child: IconButton(
            onPressed: _submitting
                ? null
                : () => _respond(ChatToolPermissionResult.allowSession),
            visualDensity: VisualDensity.compact,
            constraints: const BoxConstraints.tightFor(width: 36, height: 36),
            iconSize: 18,
            color: colorScheme.onSurfaceVariant,
            icon: const Icon(Icons.done_all_rounded),
          ),
        );
        final allowAction = FilledButton.icon(
          onPressed: _submitting
              ? null
              : () => _respond(ChatToolPermissionResult.allow),
          style: FilledButton.styleFrom(
            minimumSize: const Size(0, 36),
            padding: const EdgeInsets.symmetric(horizontal: 12),
            visualDensity: VisualDensity.compact,
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          ),
          icon: _submitting
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.check_rounded, size: 18),
          label: Text(l10n.toolApprovalAllowOnce),
        );

        if (constraints.maxWidth < 440) {
          return Row(
            children: [
              denyAction,
              Expanded(child: Center(child: allowAction)),
              sessionAction,
            ],
          );
        }
        return Row(
          children: [
            TextButton.icon(
              onPressed: _submitting
                  ? null
                  : () => _respond(ChatToolPermissionResult.deny),
              style: TextButton.styleFrom(
                minimumSize: const Size(0, 36),
                padding: const EdgeInsets.symmetric(horizontal: 8),
                visualDensity: VisualDensity.compact,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
              icon: const Icon(Icons.close_rounded, size: 18),
              label: Text(l10n.toolApprovalDeny),
            ),
            const Spacer(),
            TextButton.icon(
              onPressed: _submitting
                  ? null
                  : () => _respond(ChatToolPermissionResult.allowSession),
              style: TextButton.styleFrom(
                minimumSize: const Size(0, 36),
                padding: const EdgeInsets.symmetric(horizontal: 8),
                visualDensity: VisualDensity.compact,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
              icon: const Icon(Icons.done_all_rounded, size: 18),
              label: Text(l10n.toolApprovalAlwaysAllow),
            ),
            const SizedBox(width: 8),
            allowAction,
          ],
        );
      },
    );
  }

  /// Builds a compact approval card directly above the chat input.
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final visibleParameters = widget.request.tool.parameters
        .where(
          (parameter) =>
              parameter.name != '__operit_package_caller_name' &&
              parameter.name != '__operit_package_chat_id' &&
              parameter.name != '__operit_package_caller_card_id',
        )
        .toList(growable: false);
    return Container(
      margin: const EdgeInsets.fromLTRB(0, 6, 0, 8),
      decoration: BoxDecoration(
        color: colorScheme.surfaceContainerLow,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: colorScheme.outlineVariant),
      ),
      clipBehavior: Clip.antiAlias,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 10, 12, 8),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  Icons.admin_panel_settings_outlined,
                  size: 19,
                  color: colorScheme.primary,
                ),
                const SizedBox(width: 8),
                Text(
                  l10n.toolApprovalTitle,
                  style: theme.textTheme.labelLarge?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    widget.request.tool.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textAlign: TextAlign.end,
                    style: theme.textTheme.labelMedium?.copyWith(
                      color: colorScheme.primary,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 6),
            Text(
              widget.request.description,
              style: theme.textTheme.bodySmall?.copyWith(
                height: 1.3,
                color: colorScheme.onSurfaceVariant,
              ),
            ),
            if (visibleParameters.isNotEmpty) ...[
              const SizedBox(height: 2),
              TextButton.icon(
                onPressed: () {
                  setState(() {
                    _parametersExpanded = !_parametersExpanded;
                  });
                },
                style: TextButton.styleFrom(
                  minimumSize: const Size(0, 32),
                  padding: const EdgeInsets.symmetric(horizontal: 2),
                  visualDensity: VisualDensity.compact,
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                ),
                icon: Icon(
                  _parametersExpanded
                      ? Icons.expand_less_rounded
                      : Icons.expand_more_rounded,
                  size: 18,
                ),
                label: Text(
                  '${l10n.toolApprovalActionLabel} · ${visibleParameters.length}',
                  style: theme.textTheme.labelSmall,
                ),
              ),
              if (_parametersExpanded)
                _ChatToolPermissionParameters(parameters: visibleParameters),
            ],
            const SizedBox(height: 4),
            Divider(height: 1, color: colorScheme.outlineVariant),
            const SizedBox(height: 4),
            _buildActions(context, l10n),
          ],
        ),
      ),
    );
  }
}

class _ChatToolPermissionParameters extends StatelessWidget {
  const _ChatToolPermissionParameters({required this.parameters});

  final List<core_proxy.RuntimeHostInteractionToolPermissionToolParameter>
  parameters;

  /// Builds a compact, scrollable summary of requested tool arguments.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    return Container(
      width: double.infinity,
      constraints: const BoxConstraints(maxHeight: 120),
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: BoxDecoration(
        color: colorScheme.surfaceContainer,
        borderRadius: BorderRadius.circular(10),
      ),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final parameter in parameters)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Flexible(
                      flex: 2,
                      child: Text(
                        parameter.name,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: colorScheme.primary,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      flex: 3,
                      child: SelectableText(
                        parameter.value,
                        style: theme.textTheme.bodySmall?.copyWith(
                          height: 1.25,
                          color: colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }
}
