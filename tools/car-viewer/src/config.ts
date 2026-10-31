/** Contrat du modèle : noms de nœuds et de matériaux que le code (Three.js ici, Bevy dans le jeu) va chercher. */
export const EXPECTED_NODES: Record<string, string[]> = {
  Roues: ['steer_FL', 'steer_FR', 'spin_FL', 'spin_FR', 'spin_RL', 'spin_RR'],
  Habitacle: [
    'steering_pivot',
    'pivot_gear_lever',
    'pivot_pedal_clutch',
    'pivot_pedal_brake',
    'pivot_pedal_accel',
    'cockpit_static',
    'cam_cockpit',
  ],
  'Essuie-glaces': ['wiperpivot_L', 'wiperpivot_R'],
  Carrosserie: ['body', 'glass_rear', 'neon_body'],
  'Repères de lumière': [
    'anchor_headlight_L',
    'anchor_headlight_R',
    'anchor_underglow',
    'anchor_cabin_light',
    'anchor_dash_light',
    'anchor_front_logo_light',
    'anchor_rear_logo_light',
  ],
};

export const EXPECTED_MATERIALS = [
  'light_head',
  'light_brake',
  'signal_FL',
  'signal_FR',
  'signal_RL',
  'signal_RR',
  'neon_primary',
  'neon_secondary',
  'body_paint',
  'glass',
  'logo_rear_neon',
];

/** Rayon de roue (m) : sert à convertir km/h en rotation (valeur de CYBERCAR.md). */
export const WHEEL_RADIUS = 0.39;

/**
 * Sens de rotation du volant et des essuie-glaces. Le sens exact dépend de l'orientation des pivots dans Blender :
 * si l'animation part à l'envers sur un nouveau modèle, inverser ici plutôt que dans Blender.
 */
export const STEERING_WHEEL_SIGN = 1;
export const WIPER_SIGN = 1;
