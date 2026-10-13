import { useState } from 'react';
import { Linking, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import type { Prompt, ProviderInfo } from '../src/bridge';
import { bridge } from '../src/bridgeImpl';
import { Screen } from '../src/components/Screen';
import { t } from '../src/i18n';
import { attempt, useStore } from '../src/store';
import { colors, radii, spacing, typography } from '../src/theme';

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
      return t('mobile-account-failed', { message: session.message });
    default:
      return t('mobile-account-signed-out');
  }
}

/** The card a sign-in prompt asks the user to answer. */
function PromptCard({ slug, prompt }: { slug: string; prompt: Prompt }) {
  const [text, setText] = useState('');
  const clear = useStore((s) => s.clearPrompt);
  switch (prompt.kind) {
    case 'url':
      return (
        <View style={styles.prompt}>
          <Text style={styles.body}>{t('mobile-prompt-url')}</Text>
          <Button label={t('mobile-prompt-open')} onPress={() => attempt(Linking.openURL(prompt.url))} />
        </View>
      );
    case 'code':
      return (
        <View style={styles.prompt}>
          <Text style={styles.body}>{t('mobile-prompt-code')}</Text>
          <Text style={styles.code}>{prompt.code}</Text>
          <Button label={t('mobile-prompt-open')} onPress={() => attempt(Linking.openURL(prompt.url))} />
        </View>
      );
    case 'secret':
      return (
        <View style={styles.prompt}>
          <Text style={styles.body}>{t('mobile-prompt-secret')}</Text>
          <TextInput
            style={styles.field}
            value={text}
            onChangeText={setText}
            autoCapitalize="none"
            autoCorrect={false}
            secureTextEntry
            placeholder={t('mobile-prompt-secret-placeholder')}
            placeholderTextColor={colors.textDim}
          />
          <Button label={t('mobile-prompt-submit')} onPress={() => attempt(bridge.answer(slug, text))} />
        </View>
      );
    case 'accounts':
      return (
        <View style={styles.prompt}>
          <Text style={styles.body}>{t('mobile-prompt-accounts')}</Text>
          {prompt.choices.map((choice) => (
            <Pressable
              key={choice.id}
              style={styles.choice}
              onPress={() => attempt(bridge.answer(slug, choice.id))}
            >
              <Text style={styles.name}>{choice.name}</Text>
              {choice.detail && <Text style={styles.detail}>{choice.detail}</Text>}
            </Pressable>
          ))}
          <Button label={t('mobile-cancel')} onPress={() => clear(slug)} />
        </View>
      );
  }
}

function Button({ label, onPress }: { label: string; onPress: () => void }) {
  return (
    <Pressable style={styles.button} onPress={onPress}>
      <Text style={styles.buttonText}>{label}</Text>
    </Pressable>
  );
}

export default function Accounts() {
  const providers = useStore((s) => s.providers);
  const prompts = useStore((s) => s.prompts);
  return (
    <Screen back title={t('mobile-accounts')} miniPlayer>
      <ScrollView contentContainerStyle={styles.content}>
        {providers.map((provider) => {
          const prompt = prompts[provider.slug];
          const signedIn = provider.session.kind === 'signedIn';
          return (
            <View key={provider.slug} style={styles.card}>
              <Text style={styles.name}>{provider.name}</Text>
              <Text style={styles.detail}>{status(provider)}</Text>
              {prompt ? (
                <PromptCard slug={provider.slug} prompt={prompt} />
              ) : (
                <Button
                  label={t(signedIn ? 'mobile-sign-out' : 'mobile-sign-in')}
                  onPress={() =>
                    attempt(
                      signedIn ? bridge.signOut(provider.slug) : bridge.signIn(provider.slug, null),
                    )
                  }
                />
              )}
            </View>
          );
        })}
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing.lg, gap: spacing.md },
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
  buttonText: { color: colors.onAccent, fontWeight: '600', fontSize: typography.body },
});
