# Operit patch for dynamic_color 1.9.0

Source: published dynamic_color 1.9.0 package (material-foundation/flutter-packages).
The upstream LICENSE is retained unchanged.

## Android plugin registration

Apply `org.jetbrains.kotlin.android` in the plugin block. The published Android
script configures `kotlin.compilerOptions` but only applies `com.android.library`.
Operit uses AGP 8.11.1 with external Kotlin support (`android.builtInKotlin=false`),
so the Kotlin extension must be registered explicitly before its configuration.
No runtime behavior, platform implementation, dependency versions, or JVM target
is changed. Dart and desktop platform sources are preserved from the package.

## Pubspec constraint formatting

Remove the space between `<=` and `0.13.0` in the material_color_utilities
constraint so editor pubspec schemas recognize the range. The inclusive upper
bound and resolved dependency version remain unchanged.
