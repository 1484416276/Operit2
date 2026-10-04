// ignore_for_file: file_names

import 'package:flutter/material.dart';

import '../../../../../l10n/generated/app_localizations.dart';

/// Confirms chat-only detachment and keeps failures available for retry.
class WorkspaceUnbindDialog extends StatefulWidget {
  const WorkspaceUnbindDialog({super.key, required this.onUnbindWorkspace});

  final Future<void> Function() onUnbindWorkspace;

  @override
  State<WorkspaceUnbindDialog> createState() => _WorkspaceUnbindDialogState();
}

class _WorkspaceUnbindDialogState extends State<WorkspaceUnbindDialog> {
  bool _unbinding = false;
  String? _error;

  Future<void> _unbind() async {
    if (_unbinding) {
      return;
    }
    setState(() {
      _unbinding = true;
      _error = null;
    });
    try {
      await widget.onUnbindWorkspace();
      if (mounted) {
        Navigator.of(context).pop();
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _unbinding = false;
          _error = error.toString();
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return PopScope(
      canPop: !_unbinding,
      child: AlertDialog(
        title: Text(l10n.workspaceUnbindTitle),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Text(l10n.workspaceUnbindConfirmation),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(
                '${l10n.workspaceUnbindFailed}\n$_error',
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
        actions: <Widget>[
          TextButton(
            onPressed: _unbinding ? null : () => Navigator.of(context).pop(),
            child: Text(l10n.cancel),
          ),
          FilledButton(
            onPressed: _unbinding ? null : _unbind,
            child: _unbinding
                ? const SizedBox.square(
                    dimension: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : Text(l10n.workspaceUnbindTitle),
          ),
        ],
      ),
    );
  }
}
