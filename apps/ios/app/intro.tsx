import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import {
  Animated,
  Easing,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import type { ProviderInfo } from '../src/bridge';
import { ProviderCard } from '../src/components/ProviderCard';
import { ErrorBanner } from '../src/components/Screen';
import { t } from '../src/i18n';
import { setFlag } from '../src/preferences';
import { useReduceMotion } from '../src/settings';
import { useStore } from '../src/store';
import { makeStyles, useTheme } from '../src/styles';
import { sizes, spacing } from '../src/theme';

/** The intro's pages, in order. */
const STEPS = ['welcome', 'connect', 'done'] as const;
type Step = (typeof STEPS)[number];

/** How far a page's content travels as it rises in, and how long that takes. */
const RISE_DISTANCE = 24;
const RISE_MS = 520;
/** The pause between one rising element and the next. */
const STAGGER_MS = 90;
/** How long a page takes to slide over to the next. */
const PAGE_MS = 380;
const PAGE_SHIFT = 40;
/** The equalizer's bars: how long each takes to rise and fall, and how low it sinks. */
const BAR_PERIODS = [620, 840, 540, 760, 680];
const BAR_LOW = 0.3;
/** Where each bar holds while motion is reduced, so the mark still reads as an equalizer. */
const BAR_STILL = [0.55, 1, 0.4, 0.85, 0.7];
/** The rings that pulse out of the mark, and how long one takes to fade out. */
const RINGS = 3;
const RING_MS = 2400;
/** How slowly the glow behind each page breathes. */
const GLOW_MS = 4200;
/** The spring of a button under a finger. */
const PRESSED_SCALE = 0.96;

/** Whether a provider has an account behind it, rather than a guest session or none. */
function connected(provider: ProviderInfo): boolean {
  return provider.session.kind === 'signedIn';
}

/**
 * A value that runs `animation` on a loop from mount until unmount. While motion is reduced it
 * holds at `rest` instead.
 */
function useLoop(
  make: (value: Animated.Value) => Animated.CompositeAnimation,
  start = 0,
  rest = start,
) {
  const value = useRef(new Animated.Value(start)).current;
  const still = useReduceMotion();
  useEffect(() => {
    if (still) {
      value.setValue(rest);
      return;
    }
    value.setValue(start);
    const loop = make(value);
    loop.start();
    return () => loop.stop();
    // The animation is fixed for the life of the component; only reduced motion stops it.
  }, [still]);
  return value;
}

/** Fades and lifts its children in, `delay` milliseconds after it mounts. */
function Rise({ delay = 0, children }: { delay?: number; children: ReactNode }) {
  const still = useReduceMotion();
  const shown = useRef(new Animated.Value(still ? 1 : 0)).current;
  useEffect(() => {
    if (still) {
      shown.setValue(1);
      return;
    }
    Animated.timing(shown, {
      toValue: 1,
      duration: RISE_MS,
      delay,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start();
  }, [shown, delay, still]);
  const translateY = shown.interpolate({ inputRange: [0, 1], outputRange: [RISE_DISTANCE, 0] });
  return (
    <Animated.View style={{ opacity: shown, transform: [{ translateY }] }}>{children}</Animated.View>
  );
}

/** A soft accent glow that slowly breathes behind a page. */
function Glow() {
  const styles = useStyles();
  const breath = useLoop((value) =>
    Animated.loop(
      Animated.sequence([
        Animated.timing(value, {
          toValue: 1,
          duration: GLOW_MS,
          easing: Easing.inOut(Easing.sin),
          useNativeDriver: true,
        }),
        Animated.timing(value, {
          toValue: 0,
          duration: GLOW_MS,
          easing: Easing.inOut(Easing.sin),
          useNativeDriver: true,
        }),
      ]),
    ),
  );
  const scale = breath.interpolate({ inputRange: [0, 1], outputRange: [0.9, 1.1] });
  const opacity = breath.interpolate({ inputRange: [0, 1], outputRange: [0.7, 1] });
  return (
    <View pointerEvents="none" style={styles.glowFrame}>
      <Animated.View style={[styles.glow, { opacity, transform: [{ scale }] }]} />
      <Animated.View style={[styles.glowInner, { opacity, transform: [{ scale }] }]} />
    </View>
  );
}

/** One bar of the equalizer, rising and falling on its own beat. */
function Bar({ period, offset, rest }: { period: number; offset: number; rest: number }) {
  const styles = useStyles();
  const level = useLoop(
    (value) =>
      Animated.loop(
        Animated.sequence([
          Animated.timing(value, {
            toValue: 1,
            duration: period,
            delay: offset,
            easing: Easing.inOut(Easing.quad),
            useNativeDriver: true,
          }),
          Animated.timing(value, {
            toValue: BAR_LOW,
            duration: period,
            easing: Easing.inOut(Easing.quad),
            useNativeDriver: true,
          }),
        ]),
      ),
    BAR_LOW,
    rest,
  );
  return <Animated.View style={[styles.bar, { transform: [{ scaleY: level }] }]} />;
}

/** One ring that swells out of the mark and fades, then starts over. */
function Ring({ index }: { index: number }) {
  const styles = useStyles();
  const swell = useLoop((value) =>
    Animated.loop(
      Animated.sequence([
        Animated.delay((RING_MS / RINGS) * index),
        Animated.timing(value, {
          toValue: 1,
          duration: RING_MS,
          easing: Easing.out(Easing.quad),
          useNativeDriver: true,
        }),
        Animated.timing(value, { toValue: 0, duration: 0, useNativeDriver: true }),
      ]),
    ),
  );
  const scale = swell.interpolate({ inputRange: [0, 1], outputRange: [0.55, 1.6] });
  const opacity = swell.interpolate({ inputRange: [0, 0.2, 1], outputRange: [0, 0.55, 0] });
  return <Animated.View style={[styles.ring, { opacity, transform: [{ scale }] }]} />;
}

/** Sonora's mark for the intro: an equalizer at play, with rings pulsing out of it. */
function Mark() {
  const styles = useStyles();
  const still = useReduceMotion();
  const entrance = useRef(new Animated.Value(still ? 1 : 0)).current;
  useEffect(() => {
    if (still) {
      entrance.setValue(1);
      return;
    }
    Animated.spring(entrance, {
      toValue: 1,
      friction: 6,
      tension: 50,
      useNativeDriver: true,
    }).start();
  }, [entrance, still]);
  return (
    <Animated.View style={[styles.markFrame, { opacity: entrance, transform: [{ scale: entrance }] }]}>
      {Array.from({ length: RINGS }, (_, index) => (
        <Ring key={index} index={index} />
      ))}
      <View style={styles.mark}>
        {BAR_PERIODS.map((period, index) => (
          <Bar key={index} period={period} offset={index * STAGGER_MS} rest={BAR_STILL[index]} />
        ))}
      </View>
    </Animated.View>
  );
}

/** A check in a circle that springs in, with a ring bursting out of it. */
function Check() {
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const pop = useRef(new Animated.Value(still ? 1 : 0)).current;
  const burst = useRef(new Animated.Value(still ? 1 : 0)).current;
  useEffect(() => {
    if (still) {
      pop.setValue(1);
      burst.setValue(1);
      return;
    }
    Animated.parallel([
      Animated.spring(pop, { toValue: 1, friction: 5, tension: 70, useNativeDriver: true }),
      Animated.timing(burst, {
        toValue: 1,
        duration: RING_MS / 2,
        easing: Easing.out(Easing.quad),
        useNativeDriver: true,
      }),
    ]).start();
  }, [pop, burst, still]);
  const rotate = pop.interpolate({ inputRange: [0, 1], outputRange: ['-30deg', '0deg'] });
  const ringScale = burst.interpolate({ inputRange: [0, 1], outputRange: [0.8, 2] });
  const ringOpacity = burst.interpolate({ inputRange: [0, 0.15, 1], outputRange: [0, 0.6, 0] });
  return (
    <View style={styles.markFrame}>
      <Animated.View
        style={[styles.ring, { opacity: ringOpacity, transform: [{ scale: ringScale }] }]}
      />
      <Animated.View style={[styles.check, { transform: [{ scale: pop }, { rotate }] }]}>
        <Ionicons name="checkmark" size={sizes.introCheck / 2} color={colors.onAccent} />
      </Animated.View>
    </View>
  );
}

/** A small badge that springs onto a provider's card once it is connected. */
function Connected({ on }: { on: boolean }) {
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const shown = useRef(new Animated.Value(on ? 1 : 0)).current;
  useEffect(() => {
    if (still) {
      shown.setValue(on ? 1 : 0);
      return;
    }
    Animated.spring(shown, {
      toValue: on ? 1 : 0,
      friction: 5,
      tension: 80,
      useNativeDriver: true,
    }).start();
  }, [shown, on, still]);
  return (
    <Animated.View
      pointerEvents="none"
      style={[styles.badge, { opacity: shown, transform: [{ scale: shown }] }]}
    >
      <Ionicons name="checkmark" size={sizes.icon * 0.6} color={colors.background} />
    </Animated.View>
  );
}

/** The step dots: the current one stretches into a pill, the others wait as dots. */
function Dots({ step }: { step: number }) {
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const at = useRef(new Animated.Value(step)).current;
  useEffect(() => {
    if (still) at.setValue(step);
    else Animated.spring(at, { toValue: step, friction: 8, useNativeDriver: false }).start();
  }, [at, step, still]);
  return (
    <View style={styles.dots}>
      {STEPS.map((name, index) => {
        const width = at.interpolate({
          inputRange: [index - 1, index, index + 1],
          outputRange: [sizes.dot, sizes.dotActive, sizes.dot],
          extrapolate: 'clamp',
        });
        const backgroundColor = at.interpolate({
          inputRange: [index - 1, index, index + 1],
          outputRange: [colors.border, colors.accent, colors.border],
          extrapolate: 'clamp',
        });
        return <Animated.View key={name} style={[styles.dot, { width, backgroundColor }]} />;
      })}
    </View>
  );
}

/** A big button that gives a little under a finger. */
function Action({
  label,
  onPress,
  quiet = false,
}: {
  label: string;
  onPress: () => void;
  quiet?: boolean;
}) {
  const styles = useStyles();
  const still = useReduceMotion();
  const pressed = useRef(new Animated.Value(1)).current;
  const to = (value: number) =>
    !still &&
    Animated.spring(pressed, { toValue: value, friction: 6, tension: 200, useNativeDriver: true }).start();
  return (
    <Pressable onPress={onPress} onPressIn={() => to(PRESSED_SCALE)} onPressOut={() => to(1)}>
      <Animated.View
        style={[styles.action, quiet && styles.actionQuiet, { transform: [{ scale: pressed }] }]}
      >
        <Text style={[styles.actionText, quiet && styles.actionQuietText]}>{label}</Text>
      </Animated.View>
    </Pressable>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  const styles = useStyles();
  return (
    <View style={styles.page}>
      <View style={styles.hero}>
        <Mark />
        <Rise delay={STAGGER_MS * 3}>
          <Text style={styles.title}>{t('mobile-intro-welcome-title')}</Text>
        </Rise>
        <Rise delay={STAGGER_MS * 4}>
          <Text style={styles.body}>{t('mobile-intro-welcome-body')}</Text>
        </Rise>
      </View>
      <Rise delay={STAGGER_MS * 6}>
        <Action label={t('mobile-intro-start')} onPress={onNext} />
      </Rise>
    </View>
  );
}

function Connect({ onNext }: { onNext: () => void }) {
  const styles = useStyles();
  const providers = useStore((s) => s.providers);
  const any = providers.some(connected);
  return (
    <View style={styles.page}>
      <ScrollView
        style={styles.list}
        contentContainerStyle={styles.listContent}
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
      >
        <Rise>
          <Text style={styles.title}>{t('mobile-intro-connect-title')}</Text>
        </Rise>
        <Rise delay={STAGGER_MS}>
          <Text style={styles.body}>{t('mobile-intro-connect-body')}</Text>
        </Rise>
        {providers.map((provider, index) => (
          <Rise key={provider.slug} delay={STAGGER_MS * (index + 2)}>
            <View>
              <ProviderCard provider={provider} />
              <Connected on={connected(provider)} />
            </View>
          </Rise>
        ))}
      </ScrollView>
      <Rise delay={STAGGER_MS * (providers.length + 2)}>
        <Action
          quiet={!any}
          label={t(any ? 'mobile-intro-continue' : 'mobile-intro-later')}
          onPress={onNext}
        />
      </Rise>
    </View>
  );
}

function Done({ onFinish }: { onFinish: () => void }) {
  const { colors } = useTheme();
  const styles = useStyles();
  const providers = useStore((s) => s.providers);
  const linked = providers.filter(connected);
  return (
    <View style={styles.page}>
      <View style={styles.hero}>
        <Check />
        <Rise delay={STAGGER_MS * 3}>
          <Text style={styles.title}>{t('mobile-intro-done-title')}</Text>
        </Rise>
        <Rise delay={STAGGER_MS * 4}>
          <Text style={styles.body}>
            {t('mobile-intro-done-body', { count: linked.length })}
          </Text>
        </Rise>
        <View style={styles.chips}>
          {linked.map((provider, index) => (
            <Rise key={provider.slug} delay={STAGGER_MS * (index + 5)}>
              <View style={styles.chip}>
                <Ionicons name="checkmark-circle" size={sizes.icon * 0.7} color={colors.success} />
                <Text style={styles.chipText}>{provider.name}</Text>
              </View>
            </Rise>
          ))}
        </View>
      </View>
      <Rise delay={STAGGER_MS * (linked.length + 6)}>
        <Action label={t('mobile-intro-finish')} onPress={onFinish} />
      </Rise>
    </View>
  );
}

/**
 * The intro: a welcome, the services to sign in to, and a send-off. It shows on the first
 * launch, and again from Settings; finishing or skipping it marks it seen.
 */
export default function Intro() {
  const styles = useStyles();
  const router = useRouter();
  const [step, setStep] = useState(0);
  const page = useRef(new Animated.Value(1)).current;
  const still = useReduceMotion();

  const finish = () => {
    setFlag('introSeen', true);
    if (router.canGoBack()) router.back();
    else router.replace('/');
  };

  /** Slides the current page out to the left and the next one in from the right. */
  const go = (next: number) => {
    if (still) {
      setStep(next);
      return;
    }
    Animated.timing(page, {
      toValue: 0,
      duration: PAGE_MS / 2,
      easing: Easing.in(Easing.cubic),
      useNativeDriver: true,
    }).start(() => {
      setStep(next);
      page.setValue(-1);
      Animated.timing(page, {
        toValue: 1,
        duration: PAGE_MS,
        easing: Easing.out(Easing.cubic),
        useNativeDriver: true,
      }).start();
    });
  };

  const current: Step = STEPS[step];
  const opacity = page.interpolate({ inputRange: [-1, 0, 1], outputRange: [0, 0, 1] });
  const translateX = page.interpolate({
    inputRange: [-1, 0, 1],
    outputRange: [PAGE_SHIFT, -PAGE_SHIFT, 0],
  });

  return (
    <SafeAreaView style={styles.screen} edges={['top', 'bottom']}>
      <Glow />
      <View style={styles.top}>
        <Dots step={step} />
        {current !== 'done' && (
          <Pressable hitSlop={spacing.md} onPress={finish}>
            <Text style={styles.skip}>{t('mobile-intro-skip')}</Text>
          </Pressable>
        )}
      </View>
      <ErrorBanner />
      <Animated.View style={[styles.stage, { opacity, transform: [{ translateX }] }]}>
        {current === 'welcome' && <Welcome onNext={() => go(1)} />}
        {current === 'connect' && <Connect onNext={() => go(2)} />}
        {current === 'done' && <Done onFinish={finish} />}
      </Animated.View>
    </SafeAreaView>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  screen: { flex: 1, backgroundColor: colors.background },
  top: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.md,
  },
  skip: { color: colors.textDim, fontSize: typography.body, fontWeight: '600' },
  stage: { flex: 1 },
  page: {
    flex: 1,
    paddingHorizontal: spacing.xl,
    paddingBottom: spacing.xl,
    gap: spacing.lg,
  },
  hero: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: spacing.lg },
  title: {
    color: colors.text,
    fontSize: typography.display,
    fontWeight: '800',
    textAlign: 'center',
  },
  body: {
    color: colors.textDim,
    fontSize: typography.body,
    lineHeight: typography.body * 1.5,
    textAlign: 'center',
  },
  list: { flex: 1 },
  listContent: { gap: spacing.md, paddingTop: spacing.lg },
  glowFrame: {
    ...StyleSheet.absoluteFill,
    alignItems: 'center',
    justifyContent: 'center',
  },
  glow: {
    position: 'absolute',
    width: sizes.introGlow,
    height: sizes.introGlow,
    borderRadius: sizes.introGlow / 2,
    backgroundColor: colors.accentFaint,
  },
  glowInner: {
    position: 'absolute',
    width: sizes.introGlow / 2,
    height: sizes.introGlow / 2,
    borderRadius: sizes.introGlow / 4,
    backgroundColor: colors.accentFaint,
  },
  markFrame: {
    width: sizes.introRing,
    height: sizes.introRing,
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: spacing.xl,
  },
  mark: {
    width: sizes.introMark,
    height: sizes.introMark,
    borderRadius: sizes.introMark / 2,
    backgroundColor: colors.accentSoft,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: sizes.introBar / 2,
  },
  bar: {
    width: sizes.introBar / 2,
    height: sizes.introMark / 2,
    borderRadius: radii.pill,
    backgroundColor: colors.accent,
  },
  ring: {
    position: 'absolute',
    width: sizes.introRing,
    height: sizes.introRing,
    borderRadius: sizes.introRing / 2,
    borderWidth: 2,
    borderColor: colors.accent,
  },
  check: {
    width: sizes.introCheck,
    height: sizes.introCheck,
    borderRadius: sizes.introCheck / 2,
    backgroundColor: colors.accent,
    alignItems: 'center',
    justifyContent: 'center',
  },
  badge: {
    position: 'absolute',
    top: spacing.md,
    right: spacing.md,
    width: sizes.icon,
    height: sizes.icon,
    borderRadius: sizes.icon / 2,
    backgroundColor: colors.success,
    alignItems: 'center',
    justifyContent: 'center',
  },
  dots: { flexDirection: 'row', gap: spacing.xs + 2, alignItems: 'center' },
  dot: { height: sizes.dot, borderRadius: sizes.dot / 2 },
  action: {
    height: sizes.button,
    borderRadius: radii.pill,
    backgroundColor: colors.accent,
    alignItems: 'center',
    justifyContent: 'center',
  },
  actionQuiet: { backgroundColor: colors.surfaceRaised },
  actionText: { color: colors.onAccent, fontSize: typography.title, fontWeight: '700' },
  actionQuietText: { color: colors.text },
  chips: { flexDirection: 'row', flexWrap: 'wrap', justifyContent: 'center', gap: spacing.sm },
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs,
    backgroundColor: colors.surface,
    borderRadius: radii.pill,
    paddingHorizontal: spacing.md,
    paddingVertical: spacing.xs + 2,
  },
  chipText: { color: colors.text, fontSize: typography.caption, fontWeight: '600' },
}));
