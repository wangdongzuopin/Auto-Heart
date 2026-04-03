import { useEffect, useRef } from 'react';

type OrbState = 'idle' | 'thinking' | 'speaking';

interface OrbProps {
  onClick: () => void;
  state?: OrbState;
  size?: number;
}

const CONFIG = {
  coreRadius: 18,
  innerRadius: 7,
  ringRadius: 32,
  coreColor: '#EEEDFE',
  innerColor: '#534AB7',
  ringColor: '#AFA9EC',
  coreBorder: '#AFA9EC',
  glowColor: 'rgba(175, 169, 236, 0.40)',
  innerGlowColor: 'rgba(83, 74, 183, 0.62)',
  breathePeriod: 3200,
  thinkingPeriod: 1800,
  speakingPeriod: 1400,
  transitionSpeed: 0.16,
} as const;

type VisualState = {
  scale: number;
  alpha: number;
  ringScale: number;
  ringAlpha: number;
  rotation: number;
  glowRadius: number;
  glowAlpha: number;
  coreRadius: number;
  innerRadius: number;
};

function lerp(from: number, to: number, factor: number) {
  return from + (to - from) * factor;
}

function phase(elapsed: number, period: number) {
  return (Math.sin((elapsed / period) * Math.PI * 2 - Math.PI / 2) + 1) / 2;
}

function getTargetState(state: OrbState, elapsed: number): VisualState {
  if (state === 'thinking') {
    const p = phase(elapsed, CONFIG.thinkingPeriod);
    const t = (elapsed % CONFIG.thinkingPeriod) / CONFIG.thinkingPeriod;
    const rotation = Math.sin(t * Math.PI * 2) * (4 * Math.PI / 180);

    return {
      scale: 1,
      alpha: 0.9 + p * 0.1,
      ringScale: 0.9 + p * 0.22,
      ringAlpha: 0.16 + p * 0.18,
      rotation,
      glowRadius: 2.0 + p * 0.25,
      glowAlpha: 0.28 + p * 0.2,
      coreRadius: CONFIG.coreRadius,
      innerRadius: CONFIG.innerRadius,
    };
  }

  if (state === 'speaking') {
    const p = phase(elapsed, CONFIG.speakingPeriod);

    return {
      scale: 1,
      alpha: 0.96,
      ringScale: 1.02 + p * 0.14,
      ringAlpha: 0.08 + p * 0.08,
      rotation: 0,
      glowRadius: 1.95 + p * 0.18,
      glowAlpha: 0.34 + p * 0.12,
      coreRadius: CONFIG.coreRadius,
      innerRadius: CONFIG.innerRadius,
    };
  }

  const p = phase(elapsed, CONFIG.breathePeriod);
  return {
    scale: 1,
    alpha: 0.72 + p * 0.28,
    ringScale: 1 + p * 0.28,
    ringAlpha: 0.24 - p * 0.16,
    rotation: 0,
    glowRadius: 1.85 + p * 0.28,
    glowAlpha: 0.22 + p * 0.18,
    coreRadius: CONFIG.coreRadius,
    innerRadius: CONFIG.innerRadius,
  };
}

export default function Orb({ onClick, state = 'idle', size = 120 }: OrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const startTimeRef = useRef<number>(0);
  const stateRef = useRef<OrbState>(state);
  const visualRef = useRef<VisualState>({
    scale: 1,
    alpha: 0.9,
    ringScale: 1,
    ringAlpha: 0.18,
    rotation: 0,
    glowRadius: 2,
    glowAlpha: 0.3,
    coreRadius: CONFIG.coreRadius,
    innerRadius: CONFIG.innerRadius,
  });

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = (timestamp: number) => {
      if (!startTimeRef.current) startTimeRef.current = timestamp;
      const elapsed = timestamp - startTimeRef.current;
      const target = getTargetState(stateRef.current, elapsed);
      const current = visualRef.current;
      const blend = CONFIG.transitionSpeed;

      current.scale = lerp(current.scale, target.scale, blend);
      current.alpha = lerp(current.alpha, target.alpha, blend);
      current.ringScale = lerp(current.ringScale, target.ringScale, blend);
      current.ringAlpha = lerp(current.ringAlpha, target.ringAlpha, blend);
      current.rotation = lerp(current.rotation, target.rotation, blend);
      current.glowRadius = lerp(current.glowRadius, target.glowRadius, blend);
      current.glowAlpha = lerp(current.glowAlpha, target.glowAlpha, blend);
      current.coreRadius = lerp(current.coreRadius, target.coreRadius, blend);
      current.innerRadius = lerp(current.innerRadius, target.innerRadius, blend);

      const cx = canvas.width / 2;
      const cy = canvas.height / 2;

      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.save();
      ctx.translate(cx, cy);
      ctx.rotate(current.rotation);

      ctx.save();
      ctx.globalAlpha = Math.max(0, current.ringAlpha);
      ctx.strokeStyle = CONFIG.ringColor;
      ctx.lineWidth = stateRef.current === 'speaking' ? 1.1 : 1.4;
      ctx.beginPath();
      ctx.arc(0, 0, CONFIG.ringRadius * current.ringScale, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();

      ctx.save();
      ctx.globalAlpha = current.glowAlpha;
      const glow = ctx.createRadialGradient(
        0,
        0,
        current.coreRadius * current.scale * 0.6,
        0,
        0,
        current.coreRadius * current.scale * current.glowRadius,
      );
      glow.addColorStop(0, CONFIG.glowColor);
      glow.addColorStop(1, 'rgba(175, 169, 236, 0)');
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(0, 0, current.coreRadius * current.scale * current.glowRadius, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

      ctx.save();
      ctx.globalAlpha = current.alpha;
      ctx.fillStyle = CONFIG.coreColor;
      ctx.shadowBlur = stateRef.current === 'thinking' ? 22 : 18;
      ctx.shadowColor = CONFIG.glowColor;
      ctx.beginPath();
      ctx.arc(0, 0, current.coreRadius * current.scale, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = stateRef.current === 'thinking' ? CONFIG.innerColor : CONFIG.coreBorder;
      ctx.lineWidth = stateRef.current === 'thinking' ? 1 : 0.5;
      ctx.stroke();
      ctx.restore();

      ctx.save();
      ctx.globalAlpha = current.alpha;
      ctx.fillStyle = CONFIG.innerColor;
      ctx.shadowBlur = stateRef.current === 'speaking' ? 12 : 10;
      ctx.shadowColor = CONFIG.innerGlowColor;
      ctx.beginPath();
      ctx.arc(0, 0, current.innerRadius * current.scale, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

      ctx.restore();
      animationRef.current = requestAnimationFrame(draw);
    };

    animationRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animationRef.current);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      width={size}
      height={size}
      onClick={onClick}
      style={{
        width: size,
        height: size,
        cursor: 'pointer',
        background: 'transparent',
        display: 'block',
      }}
    />
  );
}
