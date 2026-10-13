import { useState } from 'react';
import { Alert, Linking, Pressable, Text, TextInput, View } from 'react-native';

import type { Prompt, ProviderInfo, SignInOption } from '../bridge';
import { bridge } from '../bridgeImpl';
import { t } from '../i18n';
import { attempt, signIn, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { spacing } from '../theme';

/** The status line under a provider's name. */
function status(provider: ProviderInfo): string {
  const session = provider.session;
  switch (session.kind) {
    case 'signedIn':
      return t('mobile-account-signed-in', { account: session.account });
    case 'guest':
      return t('mobile-account-guest');
    case 'connecting':
      return t('mobile-account-connecting');
    case 'expired':
      return t('mobile-account-expired');
    case 'failed':
      return session.problem
        ? t(`login-problem-${session.problem}`, { provider: provider.name })
        : t('mobile-account-failed', { message: session.message });
    default:
      return t('mobile-account-signed-out');
  }
}

/** The words on the button for one way of signing in. */
function optionLabel(provider: ProviderInfo, option: SignInOption): string {
  switch (option) {
    case 'guest':
      return t('login-guest-use');
    case 'paste':
      return t('login-connect-token');
    default:
      return t('login-sign-in', { provider: provider.name });
  }
}

/** The ways to sign in worth offering now: guest mode is left out while it is what runs. */
function offered(provider: ProviderInfo): SignInOption[] {
  return provider.options.filter(
    (option) => !(option === 'guest' && provider.session.kind === 'guest'),
  );
}

/** Asks before signing out, since the provider's songs leave the queue. */
function confirmSignOut(provider: ProviderInfo) {
  Alert.alert(
    t('mobile-sign-out-title', { provider: provider.name }),
    t('mobile-sign-out-detail'),
    [
      { text: t('mobile-cancel'), style: 'cancel' },
      {
        text: t('mobile-sign-out'),
        style: 'destructive',
        onPress: () => attempt(bridge.signOut(provider.slug)),
      },
    ],
  );
}

/** A text field with a submit button, for a pasted answer. */
function PasteField({
  placeholder,
  secure,
  onSubmit,
}: {
  placeholder: string;
  secure: boolean;
  onSubmit: (text: string) => void;
}) {
  const [text, setText] = useState('');
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <>
      <TextInput
        style={styles.field}
        value={text}
        onChangeText={setText}
        autoCapitalize="none"
        autoCorrect={false}
        secureTextEntry={secure}
        placeholder={placeholder}
        placeholderTextColor={colors.textDim}
      />
      <Button
        label={t('mobile-prompt-submit')}
        disabled={text.trim() === ''}
        onPress={() => onSubmit(text.trim())}
      />
    </>
  );
}

/** What a sign-in prompt asks of the user. Browser prompts are opened by the native side. */
function PromptBody({ slug, prompt }: { slug: string; prompt: Prompt }) {
  const styles = useStyles();
  const answer = (input: string) => attempt(bridge.answer(slug, input));
  switch (prompt.kind) {
    case 'url':
      return (
        <>
          <Text style={styles.body}>{t('mobile-prompt-url')}</Text>
          <Text style={styles.detail}>{t('mobile-prompt-url-paste')}</Text>
          <PasteField placeholder={t('mobile-prompt-url-placeholder')} secure={false} onSubmit={answer} />
        </>
      );
    case 'code':
      return (
        <>
          <Text style={styles.body}>{t('mobile-prompt-code')}</Text>
          <Text style={styles.code}>{prompt.code}</Text>
          <Button
            label={t('mobile-prompt-open')}
            onPress={() => attempt(Linking.openURL(prompt.url))}
          />
        </>
      );
    case 'secret':
      return prompt.web ? (
        <Text style={styles.body}>{t('mobile-prompt-web')}</Text>
      ) : (
        <>
          <Text style={styles.body}>{t('mobile-prompt-secret')}</Text>
          <PasteField placeholder={t('mobile-prompt-secret-placeholder')} secure onSubmit={answer} />
        </>
      );
    case 'accounts':
      return (
        <>
          <Text style={styles.body}>{t('mobile-prompt-accounts')}</Text>
          {prompt.choices.map((choice) => (
            <Pressable key={choice.id} style={styles.choice} onPress={() => answer(choice.id)}>
              <Text style={styles.name}>{choice.name}</Text>
              {choice.detail && <Text style={styles.detail}>{choice.detail}</Text>}
            </Pressable>
          ))}
        </>
      );
  }
}

function Button({
  label,
  onPress,
  quiet = false,
  disabled = false,
}: {
  label: string;
  onPress: () => void;
  quiet?: boolean;
  disabled?: boolean;
}) {
  const styles = useStyles();
  return (
    <Pressable
      style={[styles.button, quiet && styles.quiet, disabled && styles.disabled]}
      onPress={onPress}
      disabled={disabled}
    >
      <Text style={[styles.buttonText, quiet && styles.quietText]}>{label}</Text>
    </Pressable>
  );
}

/**
 * One provider: its status, and the prompt of a sign-in under way or the ways to sign in. The
 * accounts page lists one per provider, and so does the intro.
 */
export function ProviderCard({ provider }: { provider: ProviderInfo }) {
  const prompt = useStore((s) => s.prompts[provider.slug]);
  const busy = useStore((s) => s.signingIn[provider.slug] ?? false);
  const styles = useStyles();
  const signedIn = provider.session.kind === 'signedIn';
  return (
    <View style={styles.card}>
      <Text style={styles.name}>{provider.name}</Text>
      <Text style={styles.detail}>{status(provider)}</Text>
      {busy ? (
        <View style={styles.prompt}>
          {prompt && <PromptBody slug={provider.slug} prompt={prompt} />}
          <Button
            quiet
            label={t('mobile-cancel')}
            onPress={() => attempt(bridge.cancelSignIn(provider.slug))}
          />
        </View>
      ) : signedIn ? (
        <Button label={t('mobile-sign-out')} onPress={() => confirmSignOut(provider)} />
      ) : (
        <View style={styles.options}>
          {offered(provider).map((option) => (
            <Button
              key={option}
              quiet={option === 'guest' || option === 'paste'}
              label={optionLabel(provider, option)}
              onPress={() => signIn(provider, option)}
            />
          ))}
        </View>
      )}
    </View>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  card: {
    backgroundColor: colors.surface,
    borderRadius: radii.large,
    padding: spacing.lg,
    gap: spacing.sm,
  },
  name: { color: colors.text, fontSize: typography.title, fontWeight: '600' },
  detail: { color: colors.textDim, fontSize: typography.caption },
  body: { color: colors.text, fontSize: typography.body },
  code: { color: colors.accent, fontSize: typography.display, fontWeight: '700' },
  prompt: { gap: spacing.sm, marginTop: spacing.sm },
  options: { flexDirection: 'row', flexWrap: 'wrap', gap: spacing.sm },
  choice: {
    backgroundColor: colors.surfaceRaised,
    borderRadius: radii.medium,
    padding: spacing.md,
  },
  field: {
    backgroundColor: colors.surfaceRaised,
    borderRadius: radii.medium,
    padding: spacing.md,
    color: colors.text,
    fontSize: typography.body,
  },
  button: {
    alignSelf: 'flex-start',
    backgroundColor: colors.accent,
    borderRadius: radii.pill,
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.sm,
    marginTop: spacing.xs,
  },
  quiet: { backgroundColor: colors.surfaceRaised },
  disabled: { backgroundColor: colors.surfaceRaised },
  buttonText: { color: colors.onAccent, fontWeight: '600', fontSize: typography.body },
  quietText: { color: colors.text },
}));
