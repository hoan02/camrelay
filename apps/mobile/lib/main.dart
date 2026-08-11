import 'package:flutter/widgets.dart';

import 'src/api/camrelay_api.dart';
import 'src/app.dart';

void main() {
  const baseUrl = String.fromEnvironment(
    'CAMRELAY_API_BASE_URL',
    defaultValue: 'https://camrelay.example.com',
  );
  runApp(CamrelayApp(api: CamrelayApi(baseUrl: baseUrl)));
}
