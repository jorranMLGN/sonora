import { Ionicons } from '@expo/vector-icons';
import { Children, Fragment, type ReactNode, useEffect, useRef, useState } from 'react';
import { Animated, Pressable, Switch, Text, View } from 'react-native';

import { useReduceMotion } from '../settings';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';

type IconName = keyof typeof Ionicons.glyphMap;

/** How springy the thumb of a segmented control and a chosen swatch are. */
const SPRING = { friction: 9, tension: 120, useNativeDriver: true } as const;
/** How much bigger the chosen swatch is than the others. */
const SWATCH_CHOSEN_SCALE = 1.12;

/** A titled card of settings rows, with a hairline between each two. */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  const styles = useStyles();
  const rows = Children.toArray(children).filter(Boolean);
  return (
    <>
      {title && <Text style={styles.heading}>{title}</Text>}
      <View style={styles.group}>
        {rows.map((row, index) => (
          <Fragment key={index}>
            {index > 0 && <View style={styles.divider} />}
            {row}
          </Fragment>
        ))}
      </View>
    </>
  );
}

/** The icon, label and optional line under it that every row starts with. */
function Lead({
  icon,
  label,
  detail,
  danger = false,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  danger?: boolean;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <>
      <Ionicons name={icon} size={sizes.icon} color={danger ? colors.danger : colors.accent} />
      <View style={styles.rowText}>
        <Text style={[styles.label, danger && styles.danger]}>{label}</Text>
        {detail ? <Text style={styles.detail}>{detail}</Text> : null}
      </View>
    </>
  );
}

/** A row that opens something or acts on a tap, with a chevron unless it is destructive. */
export function Row({
  icon,
  label,
  detail,
  danger = false,
  onPress,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  /** Draws the row in the danger colour, for a reset. */
  danger?: boolean;
  onPress: () => void;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <Pressable style={({ pressed }) => [styles.row, pressed && styles.pressed]} onPress={onPress}>
      <Lead icon={icon} label={label} detail={detail} danger={danger} />
      {!danger && <Ionicons name="chevron-forward" size={sizes.icon * 0.75} color={colors.textDim} />}
    </Pressable>
  );
}

/** A row with an on/off switch. `locked` shows it on and greyed, for a choice the system made. */
export function SwitchRow({
  icon,
  label,
  detail,
  value,
  locked = false,
  onChange,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  value: boolean;
  locked?: boolean;
  onChange: (on: boolean) => void;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <View style={styles.row}>
      <Lead icon={icon} label={label} detail={detail} />
      <Switch
        value={value || locked}
        disabled={locked}
        onValueChange={onChange}
        trackColor={{ false: colors.border, true: colors.accent }}
        ios_backgroundColor={colors.border}
        accessibilityLabel={label}
      />
    </View>
  );
}

/** A row with a segmented control under its label, for a choice among a few. */
export function ChoiceRow<T extends string>({
  icon,
  label,
  detail,
  options,
  value,
  onChange,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const styles = useStyles();
  return (
    <View style={styles.stacked}>
      <View style={styles.stackedLead}>
        <Lead icon={icon} label={label} detail={detail} />
      </View>
      <Segmented options={options} value={value} onChange={onChange} />
    </View>
  );
}

/** Equal segments with a thumb in the accent that slides to the chosen one. */
function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const styles = useStyles();
  const still = useReduceMotion();
  const [width, setWidth] = useState(0);
  const index = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );
  const at = useRef(new Animated.Value(index)).current;
  useEffect(() => {
    if (still) at.setValue(index);
    else Animated.spring(at, { toValue: index, ...SPRING }).start();
  }, [at, index, still]);
  const segment = options.length > 0 ? width / options.length : 0;
  return (
    <View
      style={styles.segmented}
      onLayout={(event) => setWidth(event.nativeEvent.layout.width - sizes.segmentInset * 2)}
      accessibilityRole="tablist"
    >
      {segment > 0 && (
        <Animated.View
          style={[
            styles.thumb,
            { width: segment, transform: [{ translateX: Animated.multiply(at, segment) }] },
          ]}
        />
      )}
      {options.map((option) => {
        const on = option.value === value;
        return (
          <Pressable
            key={option.value}
            style={styles.segment}
            onPress={() => onChange(option.value)}
            accessibilityRole="tab"
            accessibilityState={{ selected: on }}
          >
            <Text numberOfLines={1} style={[styles.segmentText, on && styles.segmentTextOn]}>
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/** A row of colour swatches; the chosen one grows a little and wears a ring and a check. */
export function SwatchRow({
  icon,
  label,
  swatches,
  value,
  onChange,
}: {
  icon: IconName;
  label: string;
  swatches: readonly { color: string; label: string }[];
  value: string;
  onChange: (color: string) => void;
}) {
  const styles = useStyles();
  return (
    <View style={styles.stacked}>
      <View style={styles.stackedLead}>
        <Lead icon={icon} label={label} />
      </View>
      <View style={styles.swatches}>
        {swatches.map((swatch) => (
          <Swatch
            key={swatch.color}
            color={swatch.color}
            label={swatch.label}
            on={swatch.color.toLowerCase() === value.toLowerCase()}
            onPress={() => onChange(swatch.color)}
          />
        ))}
      </View>
    </View>
  );
}

function Swatch({
  color,
  label,
  on,
  onPress,
}: {
  color: string;
  label: string;
  on: boolean;
  onPress: () => void;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const scale = useRef(new Animated.Value(on ? SWATCH_CHOSEN_SCALE : 1)).current;
  useEffect(() => {
    const to = on ? SWATCH_CHOSEN_SCALE : 1;
    if (still) scale.setValue(to);
    else Animated.spring(scale, { toValue: to, ...SPRING }).start();
  }, [scale, on, still]);
  return (
    <Pressable
      onPress={onPress}
      hitSlop={spacing.xs}
      accessibilityRole="radio"
      accessibilityState={{ selected: on }}
      accessibilityLabel={label}
    >
      <Animated.View
        style={[
          styles.swatchRing,
          on && { borderColor: colors.text },
          { transform: [{ scale }] },
        ]}
      >
        <View style={[styles.swatch, { backgroundColor: color }]}>
          {on && <Ionicons name="checkmark" size={sizes.icon * 0.7} color={colors.onAccent} />}
        </View>
      </Animated.View>
    </Pressable>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  heading: {
    color: colors.textDim,
    fontSize: typography.caption,
    fontWeight: '600',
    textTransform: 'uppercase',
    marginTop: spacing.md,
    marginLeft: spacing.xs,
  },
  group: { backgroundColor: colors.surface, borderRadius: radii.large, overflow: 'hidden' },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  stacked: {
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
    gap: spacing.md,
  },
  stackedLead: { flexDirection: 'row', alignItems: 'center', gap: spacing.md },
  pressed: { backgroundColor: colors.surfaceRaised },
  rowText: { flex: 1, gap: spacing.xs / 2 },
  label: { color: colors.text, fontSize: typography.body, fontWeight: '600' },
  danger: { color: colors.danger },
  detail: { color: colors.textDim, fontSize: typography.caption },
  divider: {
    height: sizes.hairline,
    backgroundColor: colors.border,
    marginLeft: spacing.lg + sizes.icon + spacing.md,
  },
  segmented: {
    flexDirection: 'row',
    padding: sizes.segmentInset,
    borderRadius: radii.medium,
    backgroundColor: colors.surfaceRaised,
  },
  thumb: {
    position: 'absolute',
    top: sizes.segmentInset,
    bottom: sizes.segmentInset,
    left: sizes.segmentInset,
    borderRadius: Math.max(0, radii.medium - sizes.segmentInset),
    backgroundColor: colors.accent,
  },
  segment: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: spacing.sm,
    paddingHorizontal: spacing.xs,
  },
  segmentText: { color: colors.text, fontSize: typography.caption, fontWeight: '600' },
  segmentTextOn: { color: colors.onAccent },
  swatches: { flexDirection: 'row', flexWrap: 'wrap', justifyContent: 'space-between', rowGap: spacing.md },
  swatchRing: {
    width: sizes.swatch + sizes.swatchRing * 4,
    height: sizes.swatch + sizes.swatchRing * 4,
    borderRadius: radii.pill,
    borderWidth: sizes.swatchRing,
    borderColor: colors.transparent,
    alignItems: 'center',
    justifyContent: 'center',
  },
  swatch: {
    width: sizes.swatch,
    height: sizes.swatch,
    borderRadius: radii.pill,
    alignItems: 'center',
    justifyContent: 'center',
  },
}));
