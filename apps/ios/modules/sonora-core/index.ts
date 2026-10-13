import { requireNativeModule } from 'expo';

type SonoraCore = {
  version(): string;
  playTone(seconds: number): number;
};

export default requireNativeModule<SonoraCore>('SonoraCore');
