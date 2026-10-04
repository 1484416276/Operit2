// ignore_for_file: file_names

const double settingsWideLayoutBreakpoint = 760;

/// Uses the settings host width, not the full window behind navigation.
bool settingsUseWideLayout(double availableWidth) {
  return availableWidth >= settingsWideLayoutBreakpoint;
}
