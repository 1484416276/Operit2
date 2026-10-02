import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/data/preferences/UserPreferencesManager.dart';
import 'package:operit2/ui/features/packages/market/MarketBrowseList.dart';
import 'package:operit2/ui/theme/OperitTheme.dart';

/// Verifies market card text remains valid when truncation reaches an emoji.
void main() {
  testWidgets('market card preserves an emoji at the truncation boundary', (
    tester,
  ) async {
    final prefix = List<String>.filled(99, 'a').join();
    const emoji = '\u{1F600}';
    final description = '$prefix${emoji}x';

    await tester.pumpWidget(
      OperitTheme(
        initialThemePreferenceSnapshot:
            UserPreferencesManager.defaultThemePreferenceSnapshot,
        initialThemeIsReady: false,
        unconfiguredChildEnabled: true,
        hostInteractionHostsEnabled: false,
        child: Scaffold(
          body: MarketGridCard(
            title: 'Market item',
            description: description,
            author: '',
            downloads: 0,
            likes: 0,
            hearts: 0,
            actionLabel: 'Install',
            actionIcon: Icons.download_outlined,
            actionBusy: false,
            onAction: () {},
            onTap: () {},
          ),
        ),
      ),
    );

    expect(find.text('$prefix$emoji...'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
