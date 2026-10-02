// ignore_for_file: file_names

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../../l10n/generated/app_localizations.dart';
import '../runtime/RuntimeSettingsPanel.dart';

class AccessLinksSettingsPanel extends StatelessWidget {
  const AccessLinksSettingsPanel({super.key, required this.onOpenProfile});

  final VoidCallback onOpenProfile;

  /// Builds the combined device-space and browser-access settings page.
  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 20),
      children: <Widget>[
        RuntimeSettingsPanel(embedded: true, onOpenProfile: onOpenProfile),
        const SizedBox(height: 2),
        _buildWebEntry(context),
      ],
    );
  }

  /// 打开在线网页应用，不启动本地 Web Access server，也不携带 token。
  Widget _buildWebEntry(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return ListTile(
      leading: const Icon(Icons.open_in_browser_outlined),
      title: const Text('web.operit.app'),
      subtitle: Text(l10n.openInBrowser),
      onTap: () async {
        final uri = Uri.https('web.operit.app', '/');
        try {
          if (await launchUrl(uri, mode: LaunchMode.externalApplication)) {
            return;
          }
        } catch (_) {
          // 无可用浏览器时保留可复制的网页地址；不启动本地替代服务。
        }
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: SelectableText(uri.toString())),
        );
      },
    );
  }
}
