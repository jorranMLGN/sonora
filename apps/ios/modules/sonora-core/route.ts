import type { ComponentType } from 'react';
import type { ProcessedColorValue, ViewProps } from 'react-native';
import { NativeModule, requireNativeView, requireOptionalNativeModule } from 'expo';

/** What kind of device plays: the phone's own speaker, or something it sends the sound to. */
export type OutputKind = 'phone' | 'airplay' | 'bluetooth' | 'wired' | 'car' | 'other';

/** The output the audio session plays through now; `name` is the device's own name. */
export type OutputRoute = { name: string; kind: OutputKind };

type Events = {
  onRoute(route: OutputRoute): void;
};

export declare class SonoraRouteModule extends NativeModule<Events> {
  outputRoute(): OutputRoute;
}

export type RoutePickerProps = ViewProps & {
  /** The button's colour while the phone plays by itself. */
  tint?: ProcessedColorValue | null;
  /** The button's colour while another device plays. */
  activeTint?: ProcessedColorValue | null;
};

/** The output module, or null where the build does not carry it (Expo Go, the web). */
const route = requireOptionalNativeModule<SonoraRouteModule>('SonoraRoute');

export default route;

/** The system's output picker button, or null without the native module. */
export const NativeRoutePicker: ComponentType<RoutePickerProps> | null =
  route === null ? null : requireNativeView<RoutePickerProps>('SonoraRoute');
