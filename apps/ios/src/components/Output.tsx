import { useEffect, useState } from 'react';
import { processColor, Text, View } from 'react-native';

import route, { NativeRoutePicker, type OutputRoute } from '../../modules/sonora-core/route';
import { t } from '../i18n';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';

/** The output the sound goes to now, kept up as it changes; null without the native module. */
export function useOutputRoute(): OutputRoute | null {
  const [current, setCurrent] = useState<OutputRoute | null>(() => route?.outputRoute() ?? null);
  useEffect(() => {
    if (route === null) return;
    setCurrent(route.outputRoute());
    const subscription = route.addListener('onRoute', setCurrent);
    return () => subscription.remove();
  }, []);
  return current;
}

/**
 * The system's output picker: AirPlay speakers, TVs and Bluetooth devices. It takes the touch
 * for itself, so a pressable it sits in does not fire too. Nothing shows without the native
 * module.
 */
export function OutputButton({ size = sizes.icon }: { size?: number }) {
  const { colors } = useTheme();
  if (NativeRoutePicker === null) return null;
  return (
    <View onStartShouldSetResponder={() => true}>
      <NativeRoutePicker
        style={{ width: size, height: size }}
        tint={processColor(colors.text)}
        activeTint={processColor(colors.accent)}
      />
    </View>
  );
}

/** The player's output line: where the sound goes when it is not the phone, and the picker. */
export function OutputRow() {
  const output = useOutputRoute();
  const styles = useStyles();
  if (NativeRoutePicker === null) return null;
  const device = output !== null && output.kind !== 'phone' ? output.name : '';
  return (
    <View style={styles.row}>
      <Text numberOfLines={1} style={styles.label}>
        {device !== '' ? t('mobile-output-playing-on', { device }) : ''}
      </Text>
      <OutputButton />
    </View>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  row: { alignSelf: 'stretch', flexDirection: 'row', alignItems: 'center', gap: spacing.md },
  label: { flex: 1, color: colors.accent, fontSize: typography.caption, fontWeight: '600' },
}));
