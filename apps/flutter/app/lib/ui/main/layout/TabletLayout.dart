// ignore_for_file: file_names

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../components/CollapsedDrawerContent.dart';
import '../components/DrawerConversationState.dart';
import '../components/DrawerContent.dart';
import '../components/NavigationDrawerAppearance.dart';
import '../navigation/AppNavigationModels.dart';
import '../../theme/OperitGlassSurface.dart';

class TabletLayout extends StatefulWidget {
  const TabletLayout({
    super.key,
    required this.content,
    required this.navigationEntries,
    required this.pluginSidebarEntries,
    required this.selectedRouteId,
    required this.drawerConversationState,
    required this.isTabletSidebarExpanded,
    required this.tabletSidebarWidth,
    required this.collapsedTabletSidebarWidth,
    required this.onNavigationEntrySelected,
    required this.onConversationActivated,
  });

  final Widget content;
  final List<NavigationEntrySpec> navigationEntries;
  final List<NavigationEntrySpec> pluginSidebarEntries;
  final String selectedRouteId;
  final ValueListenable<DrawerConversationState> drawerConversationState;
  final bool isTabletSidebarExpanded;
  final double tabletSidebarWidth;
  final double collapsedTabletSidebarWidth;
  final ValueChanged<NavigationEntrySpec> onNavigationEntrySelected;
  final VoidCallback onConversationActivated;

  @override
  State<TabletLayout> createState() => _TabletLayoutState();
}

class _TabletLayoutState extends State<TabletLayout> {
  static const Duration _sidebarWidthAnimationDuration = Duration(
    milliseconds: 280,
  );
  static const Duration _sidebarContentFadeDuration = Duration(
    milliseconds: 160,
  );

  late bool _isSidebarWidthExpanded;
  late bool _isSidebarContentExpanded;
  double? _customSidebarWidth;
  bool _resizing = false;
  Timer? _contentSwitchTimer;
  Timer? _widthSwitchTimer;

  @override
  void initState() {
    super.initState();
    _isSidebarWidthExpanded = widget.isTabletSidebarExpanded;
    _isSidebarContentExpanded = widget.isTabletSidebarExpanded;
  }

  @override
  void didUpdateWidget(covariant TabletLayout oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.isTabletSidebarExpanded == widget.isTabletSidebarExpanded) {
      return;
    }

    _contentSwitchTimer?.cancel();
    _widthSwitchTimer?.cancel();

    if (widget.isTabletSidebarExpanded) {
      setState(() {
        _isSidebarWidthExpanded = true;
      });
      _contentSwitchTimer = Timer(_sidebarWidthAnimationDuration, () {
        if (!mounted) {
          return;
        }
        setState(() {
          _isSidebarContentExpanded = true;
        });
      });
    } else {
      setState(() {
        _isSidebarContentExpanded = false;
      });
      _widthSwitchTimer = Timer(_sidebarContentFadeDuration, () {
        if (!mounted) {
          return;
        }
        setState(() {
          _isSidebarWidthExpanded = false;
        });
      });
    }
  }

  @override
  void dispose() {
    _contentSwitchTimer?.cancel();
    _widthSwitchTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final appearance = navigationDrawerAppearanceOf(context);
    final maxExpandedWidth = math.min(
      360.0,
      math.max(260.0, MediaQuery.sizeOf(context).width * 0.36),
    );
    final expandedWidth = (_customSidebarWidth ?? widget.tabletSidebarWidth)
        .clamp(200.0, maxExpandedWidth)
        .toDouble();
    final targetSidebarWidth = _isSidebarWidthExpanded
        ? expandedWidth
        : widget.collapsedTabletSidebarWidth;

    return Row(
      children: <Widget>[
        Stack(
          clipBehavior: Clip.none,
          children: <Widget>[
            AnimatedContainer(
              duration: _resizing
                  ? Duration.zero
                  : _sidebarWidthAnimationDuration,
              curve: Curves.fastOutSlowIn,
              width: targetSidebarWidth,
              height: double.infinity,
          child: OperitGlassSurface(
            color: appearance.containerColor,
            layer: OperitGlassSurfaceLayer.panel,
            transparentAlpha: 0.035,
            borderRadius: BorderRadius.zero,
            shadows: <BoxShadow>[
              BoxShadow(
                blurRadius: 4,
                color: Colors.black.withValues(alpha: 0.12),
              ),
            ],
            child: AnimatedSwitcher(
              duration: _sidebarContentFadeDuration,
              child: _isSidebarContentExpanded
                  ? ValueListenableBuilder<DrawerConversationState>(
                      key: const ValueKey<String>('expandedSidebarContent'),
                      valueListenable: widget.drawerConversationState,
                      builder: (context, drawerState, _) {
                        return DrawerContent(
                          key: const ValueKey<String>('expandedDrawerContent'),
                          navigationEntries: widget.navigationEntries,
                          pluginEntries: widget.pluginSidebarEntries,
                          selectedRouteId: widget.selectedRouteId,
                          appearance: appearance,
                          histories: drawerState.histories,
                          activeStreamingChatIds:
                              drawerState.activeStreamingChatIds,
                          characterGroupNamesById:
                              drawerState.characterGroupNamesById,
                          characterCardAvatarUrisByName:
                              drawerState.characterCardAvatarUrisByName,
                          currentChatId: drawerState.currentChatId,
                          errorMessage: drawerState.errorMessage,
                          loading: drawerState.loading,
                          onNavigationEntrySelected:
                              widget.onNavigationEntrySelected,
                          onConversationActivated:
                              widget.onConversationActivated,
                        );
                      },
                    )
                  : CollapsedDrawerContent(
                      key: const ValueKey<String>('collapsedSidebarContent'),
                      navigationEntries: widget.navigationEntries,
                      pluginEntries: widget.pluginSidebarEntries,
                      selectedRouteId: widget.selectedRouteId,
                      appearance: appearance,
                      onNavigationEntrySelected:
                          widget.onNavigationEntrySelected,
                      onConversationActivated: widget.onConversationActivated,
                    ),
                ),
              ),
            ),
            if (_isSidebarWidthExpanded)
              PositionedDirectional(
                top: 0,
                bottom: 0,
                end: -4,
                width: 8,
                child: MouseRegion(
                  cursor: SystemMouseCursors.resizeColumn,
                  child: GestureDetector(
                    behavior: HitTestBehavior.translucent,
                    onHorizontalDragStart: (_) {
                      setState(() {
                        _resizing = true;
                      });
                    },
                    onHorizontalDragUpdate: (details) {
                      setState(() {
                        final current =
                            _customSidebarWidth ?? widget.tabletSidebarWidth;
                        _customSidebarWidth = (current + details.delta.dx)
                            .clamp(200.0, maxExpandedWidth)
                            .toDouble();
                      });
                    },
                    onHorizontalDragEnd: (_) {
                      if (!_resizing) {
                        return;
                      }
                      setState(() {
                        _resizing = false;
                      });
                    },
                    onDoubleTap: () {
                      setState(() {
                        _customSidebarWidth = null;
                      });
                    },
                  ),
                ),
              ),
          ],
        ),
        Expanded(child: widget.content),
      ],
    );
  }
}
