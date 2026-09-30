// ignore_for_file: file_names

import 'package:flutter/material.dart';

import 'WorkspaceTabModels.dart';

/// Describes the orientation of the two workspace panes.
enum WorkspaceSplitAxis { horizontal, vertical }

/// Keeps workspace navigation alive independently of mounted panel widgets.
class WorkspaceSession {
  /// Creates the main workspace with its permanent home tab selected.
  WorkspaceSession();

  final List<WorkspaceTab> tabs = <WorkspaceTab>[
    const WorkspaceTab(
      kind: WorkspaceTabKind.home,
      title: '',
      icon: Icons.home_outlined,
      closable: false,
    ),
  ];
  final List<WorkspaceTab> secondaryTabs = <WorkspaceTab>[];
  int selectedIndex = 0;
  int secondarySelectedIndex = 0;
  WorkspaceSplitAxis? splitAxis;
  double splitRatio = 0.5;
  bool secondaryPaneFirst = false;
  int tabIdentitySequence = 0;
}
