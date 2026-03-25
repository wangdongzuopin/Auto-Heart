import { useEffect, useRef, useCallback } from 'react';
import { isTauriRuntime } from '../tauriRuntime';

/**
 * Orb 组件 — Auto-Heart 的具身形态
 *
 * 三层同心圆结构（匹配 prototype 规范）：
 *   外环 Ring  — 64px, #AFA9EC 1.5px border, 脉冲扩散
 *   核心 Core  — 36px, #EEEDFE 填充, #AFA9EC 0.5px border
 *   内芯 Inner — 14px, #534AB7 填充
 *
 * 三种状态：
 *   idle     — 呼吸动画: scale 1.0→1.22, opacity 0.55→1.0, 周期 3.2s
 *   thinking — 不规则跳动: 旋转 ±5°, scale 抖动, 周期 2s
 *   speaking — 核心缩小到 32px, 外表现交由父组件气泡处理
 */

type OrbState = 'idle' | 'thinking' | 'speaking';

interface OrbProps {
  onClick: () => void;
  state?: OrbState;
  size?: number; // 画布尺寸，默认 120
}

// Orb 配置 — 严格对齐 prototype HTML 参数
const CONFIG = {
  // 尺寸（基于 prototype 原始值, 相对 64px ring）
  coreRadius: 18,      // 核心半径 36/2
  innerRadius: 7,      // 内芯半径 14/2
  ringRadius: 32,      // 外环半径 64/2

  // 颜色
  coreColor: '#EEEDFE',
  innerColor: '#534AB7',
  ringColor: '#AFA9EC',
  coreBorder: '#AFA9EC',

  // 动画
  breathePeriod: 3200,     // ms
  pulseRingPeriod: 3200,
  thinkingPeriod: 2000,

  // 发光
  glowColor: 'rgba(175, 169, 236, 0.4)',
  innerGlowColor: 'rgba(83, 74, 183, 0.6)',
} as const;

export default function Orb({ onClick, state = 'idle', size = 120 }: OrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const startTimeRef = useRef<number>(0);

  // 正弦相位 0→1
  const phase = useCallback((elapsed: number, period: number) => {
    return (Math.sin((elapsed / period) * Math.PI * 2 - Math.PI / 2) + 1) / 2;
  }, []);

  const draw = useCallback((timestamp: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (!startTimeRef.current) startTimeRef.current = timestamp;
    const elapsed = timestamp - startTimeRef.current;

    const cx = canvas.width / 2;
    const cy = canvas.height / 2;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    if (state === 'idle') {
      // ====== 沉默态：呼吸 ======
      const p = phase(elapsed, CONFIG.breathePeriod);
      const scale = 1 + p * 0.22;        // 1.0 → 1.22
      const alpha = 0.55 + p * 0.45;     // 0.55 → 1.0

      // 脉冲环 — 从核心扩散到 2.2x，逐渐消失
      const ringP = phase(elapsed, CONFIG.pulseRingPeriod);
      const ringScale = 1 + ringP * 1.2;   // 1.0 → 2.2
      const ringAlpha = 0.4 * (1 - ringP); // 0.4 → 0

      ctx.save();
      ctx.globalAlpha = ringAlpha;
      ctx.strokeStyle = CONFIG.ringColor;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(cx, cy, CONFIG.ringRadius * ringScale, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();

      // 外层柔和光晕
      ctx.save();
      ctx.globalAlpha = alpha * 0.5;
      const glow = ctx.createRadialGradient(cx, cy, CONFIG.coreRadius * scale * 0.8, cx, cy, CONFIG.coreRadius * scale * 1.8);
      glow.addColorStop(0, CONFIG.glowColor);
      glow.addColorStop(1, 'rgba(175, 169, 236, 0)');
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(cx, cy, CONFIG.coreRadius * scale * 1.8, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

      // 核心圆
      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.fillStyle = CONFIG.coreColor;
      ctx.shadowBlur = 15;
      ctx.shadowColor = CONFIG.glowColor;
      ctx.beginPath();
      ctx.arc(cx, cy, CONFIG.coreRadius * scale, 0, Math.PI * 2);
      ctx.fill();
      // 核心边框
      ctx.strokeStyle = CONFIG.coreBorder;
      ctx.lineWidth = 0.5;
      ctx.stroke();
      ctx.restore();

      // 内芯
      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.fillStyle = CONFIG.innerColor;
      ctx.shadowBlur = 10;
      ctx.shadowColor = CONFIG.innerGlowColor;
      ctx.beginPath();
      ctx.arc(cx, cy, CONFIG.innerRadius * scale, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

    } else if (state === 'thinking') {
      // ====== 思考态：不规则跳动 ======
      const p = phase(elapsed, CONFIG.thinkingPeriod);
      const t = (elapsed % CONFIG.thinkingPeriod) / CONFIG.thinkingPeriod;

      // 旋转 ±5°
      const rotation = Math.sin(t * Math.PI * 2) * (5 * Math.PI / 180);
      // scale 抖动
      let scaleVal: number;
      if (t < 0.33) {
        scaleVal = 1 + (t / 0.33) * 0.1;        // 1.0 → 1.1
      } else if (t < 0.66) {
        scaleVal = 1.1 - ((t - 0.33) / 0.33) * 0.15; // 1.1 → 0.95
      } else {
        scaleVal = 0.95 + ((t - 0.66) / 0.34) * 0.05; // 0.95 → 1.0
      }
      const alpha = 0.6 + p * 0.4;

      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.translate(cx, cy);
      ctx.rotate(rotation);

      // 加速的外环闪烁
      ctx.strokeStyle = CONFIG.innerColor;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(0, 0, CONFIG.ringRadius * scaleVal * 0.85, 0, Math.PI * 2);
      ctx.stroke();

      // 核心 — 放大到 40px（半径 20）
      ctx.fillStyle = CONFIG.coreColor;
      ctx.shadowBlur = 25;
      ctx.shadowColor = CONFIG.innerGlowColor;
      ctx.beginPath();
      ctx.arc(0, 0, 20 * scaleVal, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = CONFIG.innerColor;
      ctx.lineWidth = 1;
      ctx.stroke();

      // 内芯 — 16px 直径
      ctx.shadowBlur = 12;
      ctx.fillStyle = CONFIG.innerColor;
      ctx.beginPath();
      ctx.arc(0, 0, 8 * scaleVal, 0, Math.PI * 2);
      ctx.fill();

      ctx.restore();

    } else if (state === 'speaking') {
      // ====== 发言态：核心缩小到 32px (半径 16)，稳定发光 ======
      ctx.save();

      // 柔和光晕
      const speakGlow = ctx.createRadialGradient(cx, cy, 10, cx, cy, 35);
      speakGlow.addColorStop(0, 'rgba(175, 169, 236, 0.5)');
      speakGlow.addColorStop(1, 'rgba(175, 169, 236, 0)');
      ctx.fillStyle = speakGlow;
      ctx.beginPath();
      ctx.arc(cx, cy, 35, 0, Math.PI * 2);
      ctx.fill();

      // 核心
      ctx.fillStyle = CONFIG.coreColor;
      ctx.shadowBlur = 20;
      ctx.shadowColor = CONFIG.innerGlowColor;
      ctx.beginPath();
      ctx.arc(cx, cy, 16, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = CONFIG.innerColor;
      ctx.lineWidth = 1;
      ctx.stroke();

      // 内芯
      ctx.shadowBlur = 10;
      ctx.fillStyle = CONFIG.innerColor;
      ctx.beginPath();
      ctx.arc(cx, cy, 6, 0, Math.PI * 2);
      ctx.fill();

      ctx.restore();
    }

    animationRef.current = requestAnimationFrame(draw);
  }, [state, phase]);

  // 启动动画循环
  useEffect(() => {
    startTimeRef.current = 0;
    animationRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animationRef.current);
  }, [draw]);

  // 监听 Rust 心跳事件
  useEffect(() => {
    let unlistenShallow: (() => void) | undefined;
    let unlistenMiddle: (() => void) | undefined;

    const setup = async () => {
      if (!isTauriRuntime()) return;
      const { listen } = await import('@tauri-apps/api/event');

      unlistenShallow = await listen('heartbeat:shallow', () => {
        console.log('[Orb] shallow heartbeat');
      });

      unlistenMiddle = await listen('heartbeat:middle', () => {
        console.log('[Orb] middle heartbeat');
      });
    };

    setup();

    return () => {
      unlistenShallow?.();
      unlistenMiddle?.();
    };
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
