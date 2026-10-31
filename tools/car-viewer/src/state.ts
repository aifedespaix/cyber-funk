export type Blinker = 'off' | 'left' | 'right' | 'hazard';
export type Wipers = 'off' | 'intermittent' | 'slow' | 'fast';
export type Gear = 'R' | 'N' | '1' | '2' | '3' | '4' | '5' | '6';

/** État piloté par l'interface : il survit au rechargement du modèle. */
export interface AppState {
  steering: number; // -1 (droite) … +1 (gauche)
  speedKmh: number;
  clutch: number;
  brake: number;
  throttle: number;
  gear: Gear;
  wipers: Wipers;
  blinker: Blinker;
  headlights: boolean;
  highBeams: boolean;
  braking: boolean; // frein forcé, en plus de la pédale
  neonPrimary: string;
  neonSecondary: string;
  neonIntensity: number;
  glassOpacity: number;
  hoverOn: boolean;
  hoverHeight: number;
  hoverIntensity: number;
  hoverColor: string;
  hoverFollowNeon: boolean;
  fps: boolean;
  fov: number;
  bloom: number;
  bloomThreshold: number;
  exposure: number;
  showMarkers: boolean;
}

export function defaultState(): AppState {
  return {
    steering: 0,
    speedKmh: 0,
    clutch: 0,
    brake: 0,
    throttle: 0,
    gear: 'N',
    wipers: 'off',
    blinker: 'off',
    headlights: false,
    highBeams: false,
    braking: false,
    neonPrimary: '#00e5ff',
    neonSecondary: '#ff0d73',
    neonIntensity: 1,
    glassOpacity: 0.28,
    hoverOn: false,
    hoverHeight: 0.45,
    hoverIntensity: 1,
    hoverColor: '#ffffff',
    hoverFollowNeon: true,
    fps: false,
    fov: 75,
    bloom: 0.6,
    bloomThreshold: 0.9,
    exposure: 1,
    showMarkers: false,
  };
}
