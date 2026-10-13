import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect } from 'react';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { useReduceMotion } from '../src/settings';
import { startStore } from '../src/store';
import { useTheme } from '../src/styles';

export default function RootLayout() {
  useEffect(startStore, []);
  const { colors } = useTheme();
  const still = useReduceMotion();
  return (
    <SafeAreaProvider>
      <StatusBar style="light" />
      <Stack
        screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.background } }}
      >
        <Stack.Screen name="(tabs)" />
        <Stack.Screen name="player" options={{ presentation: 'modal' }} />
        <Stack.Screen name="intro" options={{ animation: still ? 'none' : 'fade', gestureEnabled: false }} />
      </Stack>
    </SafeAreaProvider>
  );
}
