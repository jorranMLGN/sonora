import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect } from 'react';
import { StyleSheet } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { Toast } from '../src/components/Screen';
import { useReduceMotion } from '../src/settings';
import { startStore } from '../src/store';
import { useTheme } from '../src/styles';

export default function RootLayout() {
  useEffect(startStore, []);
  const { colors } = useTheme();
  const still = useReduceMotion();
  return (
    <GestureHandlerRootView style={styles.root}>
      <SafeAreaProvider>
        <StatusBar style="light" />
        <Stack
          screenOptions={{
            headerShown: false,
            contentStyle: { backgroundColor: colors.background },
            // Pages slide in as iOS's own do; Reduce Motion fades them instead.
            animation: still ? 'fade' : 'default',
          }}
        >
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="player" options={{ presentation: 'modal' }} />
          <Stack.Screen
            name="intro"
            options={{ animation: still ? 'none' : 'fade', gestureEnabled: false }}
          />
        </Stack>
        <Toast />
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1 },
});
