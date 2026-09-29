// ignore_for_file: file_names

import 'package:flutter/widgets.dart';

import '../../../main/layout/NavigationLayoutMetrics.dart';

const double settingsWideLayoutBreakpoint = navigationTabletBreakpoint;

bool settingsUseWideLayout(BuildContext context) {
  return MediaQuery.sizeOf(context).width >= settingsWideLayoutBreakpoint;
}
