// ignore_for_file: file_names

import 'package:flutter/material.dart';

import '../../../../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../../../../core/proxy/generated/CoreProxyModels.g.dart';
import '../../../../../l10n/generated/app_localizations.dart';

/// Selects an owner-scoped folder and previews its contents before attachment.
class MemoryAttachmentDialog extends StatefulWidget {
  const MemoryAttachmentDialog({super.key, required this.clients});

  final GeneratedCoreProxyClients clients;

  @override
  State<MemoryAttachmentDialog> createState() => _MemoryAttachmentDialogState();
}

class _MemoryAttachmentDialogState extends State<MemoryAttachmentDialog> {
  List<({String key, String name})> _owners = [];
  List<String>? _folders;
  ({String key, String name})? _owner;
  String? _folder;
  String? _preview;
  Object? _error;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _loadOwners();
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _loadOwners() => _run(() async {
    final cards = await widget.clients.preferencesCharacterCardManager
        .getAllCharacterCards();
    final stores = await widget.clients.preferencesSharedMemoryStoreManager
        .getAllSharedMemoryStores();
    if (!mounted) return;
    _owners = [
      for (final card in cards) (key: 'character:${card.id}', name: card.name),
      for (final store in stores) (key: 'shared:${store.id}', name: store.name),
    ];
  });

  Future<void> _loadFolders(({String key, String name}) owner) =>
      _run(() async {
        _owner = owner;
        final folders = await widget.clients
            .repositoryMemoryRepositoryForOwner(owner.key)
            .getAllFolderPaths();
        if (!mounted) return;
        _owner = owner;
        _folders = {'', ...folders}.toList()..sort();
      });

  Future<void> _loadPreview(String folder) => _run(() async {
    final owner = _owner!;
    final List<Memory> memories = await widget.clients
        .repositoryMemoryRepositoryForOwner(owner.key)
        .getMemoriesByFolderPath(folderPath: folder);
    if (!mounted) return;
    _folder = folder;
    _preview = memories.isEmpty
        ? ''
        : '# ${owner.name} / ${folder.isEmpty ? '/' : folder}\n\n${memories.map((memory) => '## ${memory.title}\n\n${memory.content}').join('\n\n')}';
  });

  void _back() {
    setState(() {
      _error = null;
      if (_folder != null && _folders != null) {
        _preview = null;
        _folder = null;
      } else {
        _owner = null;
        _folders = null;
        _folder = null;
        _preview = null;
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return AlertDialog(
      title: Row(
        children: [
          if (_owner != null && !_loading)
            IconButton(onPressed: _back, icon: const Icon(Icons.arrow_back)),
          Expanded(child: Text(_owner?.name ?? l10n.attachmentMemory)),
        ],
      ),
      content: SizedBox(
        width: 480,
        height: 360,
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
            ? Column(
                children: [
                  Expanded(
                    child: SingleChildScrollView(child: Text('$_error')),
                  ),
                  TextButton(
                    onPressed: () {
                      if (_folder != null) {
                        _loadPreview(_folder!);
                      } else if (_owner != null) {
                        _loadFolders(_owner!);
                      } else {
                        _loadOwners();
                      }
                    },
                    child: Text(l10n.retry),
                  ),
                ],
              )
            : _preview != null
            ? SingleChildScrollView(
                child: SelectableText(
                  _preview!.isEmpty ? l10n.emptyFolder : _preview!,
                ),
              )
            : _folders != null
            ? ListView(
                children: [
                  for (final folder in _folders!)
                    ListTile(
                      leading: const Icon(Icons.folder_outlined),
                      title: Text(folder.isEmpty ? '/' : folder),
                      onTap: () {
                        _folder = folder;
                        _loadPreview(folder);
                      },
                    ),
                ],
              )
            : _owners.isEmpty
            ? Center(child: Text(l10n.noData))
            : ListView(
                children: [
                  for (final owner in _owners)
                    ListTile(
                      leading: Icon(
                        owner.key.startsWith('shared:')
                            ? Icons.people_outline
                            : Icons.person_outline,
                      ),
                      title: Text(owner.name),
                      onTap: () => _loadFolders(owner),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
        if (!_loading && _error == null && (_preview?.isNotEmpty ?? false))
          FilledButton.icon(
            onPressed: () => Navigator.pop(context, _preview),
            icon: const Icon(Icons.attach_file),
            label: Text(l10n.attachmentMemory),
          ),
      ],
    );
  }
}
