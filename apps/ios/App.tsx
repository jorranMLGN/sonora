import { StatusBar } from 'expo-status-bar';
import { useState } from 'react';
import { Button, StyleSheet, Text, View } from 'react-native';

import SonoraCore from './modules/sonora-core';

export default function App() {
  const [result, setResult] = useState<string>('');

  return (
    <View style={styles.container}>
      <Text style={styles.title}>{SonoraCore.version()}</Text>
      <Button
        title="Play tone"
        onPress={() => setResult(`play_tone returned ${SonoraCore.playTone(3)}`)}
      />
      <Text>{result}</Text>
      <StatusBar style="auto" />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#fff',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 16,
  },
  title: {
    fontSize: 18,
  },
});
