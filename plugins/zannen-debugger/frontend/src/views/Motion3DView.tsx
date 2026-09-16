/**
 * 3D 运动渲染：跟踪器姿态的实时三维视图。
 *
 * - three.js 场景：跟踪器模型（板形网格 + 发光描边）+ 参考网格 + 灯光；
 * - 数据源：imu.batch 最新四元数，按选中设备 label 过滤，逐帧 slerp 平滑逼近；
 * - 标定：把当前姿态设为零点（四元数偏移）；
 * - 组件卸载时完整释放 WebGL 资源。
 */

import {
  Button,
  EmptyState,
  GlassCard,
  Segmented,
  SectionTitle,
  useBusEvent,
  useLocale,
  useTheme,
} from "@zannen/plugin-sdk";
import { Box, Crosshair } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { sampleDevice, type ImuSample } from "../buffers";
import { t, tf } from "../i18n";
import { useConnection } from "../state";

export function Motion3DView() {
  const locale = useLocale();
  const conn = useConnection();
  const theme = useTheme();
  const hostRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<THREE.Scene | null>(null);
  const gridRef = useRef<THREE.GridHelper | null>(null);
  const targetQuatRef = useRef(new THREE.Quaternion());
  const offsetRef = useRef(new THREE.Quaternion());
  const [streaming, setStreaming] = useState(false);
  const [quatText, setQuatText] = useState("-");
  const quatTextRef = useRef("-");
  const lastSampleTsRef = useRef(0);
  const [selected, setSelected] = useState<string | null>(null);
  const selectedRef = useRef<string | null>(null);

  // 设备选择器：选项来自已连接会话（label 即样本的 device 字段）
  const deviceLabels = [...new Set(conn.sessions.map((s) => s.label))];
  const sel =
    selected && deviceLabels.includes(selected)
      ? selected
      : conn.label && deviceLabels.includes(conn.label)
        ? conn.label
        : (deviceLabels[0] ?? null);
  selectedRef.current = sel;

  useBusEvent<{ samples: ImuSample[] }>("zannen.debugger/imu.batch", ({ samples }) => {
    const want = selectedRef.current;
    if (want === null) return;
    const fallback = useConnection.getState().label ?? "";
    const latest = [...samples].reverse().find((s) => s.quat && sampleDevice(s, fallback) === want);
    if (!latest?.quat) return;
    const [w, x, y, z] = latest.quat;
    targetQuatRef.current.set(x, y, z, w).normalize();
    lastSampleTsRef.current = Date.now();
    quatTextRef.current = `w=${w.toFixed(3)} x=${x.toFixed(3)} y=${y.toFixed(3)} z=${z.toFixed(3)}`;
  });

  // 高频样本只进 ref；读数与流状态低频（4Hz）同步到 state
  useEffect(() => {
    const timer = setInterval(() => {
      setQuatText(quatTextRef.current);
      setStreaming(Date.now() - lastSampleTsRef.current <= 1500);
    }, 250);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(42, host.clientWidth / host.clientHeight, 0.1, 100);
    camera.position.set(0, 1.6, 3.4);
    camera.lookAt(0, 0, 0);

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(host.clientWidth, host.clientHeight);
    host.appendChild(renderer.domElement);

    // 跟踪器：扁平 PCB 板造型
    const tracker = new THREE.Group();
    const board = new THREE.Mesh(
      new THREE.BoxGeometry(0.9, 0.08, 0.6),
      new THREE.MeshStandardMaterial({ color: 0x2a2f3d, roughness: 0.55, metalness: 0.25 }),
    );
    const edges = new THREE.LineSegments(
      new THREE.EdgesGeometry(board.geometry),
      new THREE.LineBasicMaterial({ color: 0x6e8bff }),
    );
    const chip = new THREE.Mesh(
      new THREE.BoxGeometry(0.24, 0.05, 0.24),
      new THREE.MeshStandardMaterial({
        color: 0x6e8bff,
        emissive: 0x22336b,
        roughness: 0.3,
      }),
    );
    chip.position.y = 0.07;
    // 方向指示箭头（前向 -Z 染成强调色小锥）
    const nose = new THREE.Mesh(
      new THREE.ConeGeometry(0.06, 0.16, 4),
      new THREE.MeshStandardMaterial({ color: 0x9b7bff, emissive: 0x2a1d55 }),
    );
    nose.rotation.x = -Math.PI / 2;
    nose.position.set(0, 0.06, -0.38);
    tracker.add(board, edges, chip, nose);
    scene.add(tracker);

    const grid = new THREE.GridHelper(6, 12, 0x39415a, 0x232838);
    grid.position.y = -0.6;
    scene.add(grid);
    sceneRef.current = scene;
    gridRef.current = grid;
    scene.add(new THREE.AmbientLight(0xffffff, 0.55));
    const key = new THREE.DirectionalLight(0xffffff, 1.1);
    key.position.set(2.5, 4, 3);
    scene.add(key);

    const ro = new ResizeObserver(() => {
      camera.aspect = host.clientWidth / host.clientHeight;
      camera.updateProjectionMatrix();
      renderer.setSize(host.clientWidth, host.clientHeight);
    });
    ro.observe(host);

    let raf = 0;
    const current = new THREE.Quaternion();
    const animate = () => {
      raf = requestAnimationFrame(animate);
      // slerp 平滑 + 标定偏移
      const target = targetQuatRef.current.clone().premultiply(offsetRef.current);
      current.slerp(target, 0.14);
      tracker.quaternion.copy(current);
      // 无数据时缓慢自转展示
      if (Date.now() - lastSampleTsRef.current > 1500) {
        tracker.rotation.y += 0.004;
      }
      renderer.render(scene, camera);
    };
    animate();

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      scene.traverse((obj) => {
        if (obj instanceof THREE.Mesh || obj instanceof THREE.LineSegments) {
          obj.geometry.dispose();
          const mat = obj.material as THREE.Material | THREE.Material[];
          (Array.isArray(mat) ? mat : [mat]).forEach((m) => m.dispose());
        }
      });
      renderer.dispose();
      host.removeChild(renderer.domElement);
    };
  }, []);

  const calibrate = () => {
    offsetRef.current.copy(targetQuatRef.current).invert();
  };

  // 主题切换：替换参考网格配色（GridHelper 顶点色只能重建）
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    if (gridRef.current) {
      scene.remove(gridRef.current);
      gridRef.current.geometry.dispose();
      (gridRef.current.material as THREE.Material).dispose();
    }
    const dark = theme === "dark";
    const grid = new THREE.GridHelper(6, 12, dark ? 0x39415a : 0xa8b0c8, dark ? 0x232838 : 0xd6dbe8);
    grid.position.y = -0.6;
    scene.add(grid);
    gridRef.current = grid;
  }, [theme]);

  return (
    <div className="zd-view zd-motion3d-wrap">
      <SectionTitle
        title={t(locale, "motion.title")}
        desc={sel ? tf(locale, "motion.descConnected", { label: sel }) : t(locale, "motion.descIdle")}
      />
      <div className="zd-toolbar">
        {deviceLabels.length > 1 && sel !== null && (
          <>
            <span className="zd-toolbar-label">{t(locale, "timeline.source")}</span>
            <Segmented
              options={deviceLabels.map((l) => ({ value: l, label: l }))}
              value={sel}
              onChange={setSelected}
            />
          </>
        )}
        <Button variant="ghost" onClick={calibrate}>
          <Crosshair size={13} /> {t(locale, "motion.calibrate")}
        </Button>
        <span className={`zd-stream-badge${streaming ? " is-live" : ""}`}>
          {streaming ? t(locale, "motion.live") : t(locale, "motion.waiting")}
        </span>
        <span className="zd-quat-readout">{quatText}</span>
      </div>
      <GlassCard padded={false} className="zd-motion3d-canvas-card">
        <div ref={hostRef} className="zd-motion3d-canvas" />
        {conn.session === null && (
          <div className="zd-motion3d-overlay">
            <EmptyState
              icon={<Box size={24} />}
              title={t(locale, "motion.overlayTitle")}
              desc={t(locale, "motion.overlayDesc")}
            />
          </div>
        )}
      </GlassCard>
    </div>
  );
}
