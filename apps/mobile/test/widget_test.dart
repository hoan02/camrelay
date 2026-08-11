import 'package:camrelay_mobile/src/api/camrelay_api.dart';
import 'package:camrelay_mobile/src/app.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('renders the Camrelay login surface', (tester) async {
    await tester.pumpWidget(
      CamrelayApp(api: CamrelayApi(baseUrl: 'https://example.test')),
    );

    expect(find.text('camrelay'), findsOneWidget);
    expect(find.text('Private camera gateway'), findsOneWidget);
    expect(find.byType(TextField), findsNWidgets(2));
  });
}
