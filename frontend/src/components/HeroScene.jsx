import { useEffect, useRef } from 'react'
import * as THREE from 'three'

export default function HeroScene() {
  const containerRef = useRef(null)

  useEffect(() => {
    const el = containerRef.current
    if (!el) return

    const renderer = new THREE.WebGLRenderer({ antialias: true })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    renderer.setSize(el.clientWidth, el.clientHeight)
    renderer.setClearColor(0x09090b, 1)
    el.appendChild(renderer.domElement)

    const scene  = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(50, el.clientWidth / el.clientHeight, 0.1, 100)
    camera.position.set(0, 0.8, 4.2)
    camera.lookAt(0, 0, 0)

    // Sun off-screen to camera-left: strong warm directional creates a clear
    // day/night terminator on Earth (iconic "Earth-from-orbit" lighting).
    const SUN_DIR = new THREE.Vector3(-6, 1.2, 2).normalize()
    const sun = new THREE.DirectionalLight(0xfff2d0, 2.4)
    sun.position.copy(SUN_DIR).multiplyScalar(6)
    scene.add(sun)
    // Faint cool ambient so the dark hemisphere isn't pitch black.
    scene.add(new THREE.AmbientLight(0x1a2436, 0.5))
    // Tiny cool fill from the right hints at Earth-reflected light.
    const fill = new THREE.DirectionalLight(0x2a3b58, 0.3)
    fill.position.set(4, -1, -2)
    scene.add(fill)

    const STAR_COUNT = 2000
    const starPos    = new Float32Array(STAR_COUNT * 3)
    for (let i = 0; i < STAR_COUNT * 3; i++) starPos[i] = (Math.random() - 0.5) * 80
    const starGeo = new THREE.BufferGeometry()
    starGeo.setAttribute('position', new THREE.BufferAttribute(starPos, 3))
    scene.add(new THREE.Points(starGeo, new THREE.PointsMaterial({ color: 0xffffff, size: 0.07, sizeAttenuation: true })))

    const world = new THREE.Group()
    world.position.set(0, 0, 0)
    scene.add(world)

    const earth = new THREE.Mesh(
      new THREE.SphereGeometry(1, 64, 64),
      new THREE.MeshPhongMaterial({ shininess: 8, specular: new THREE.Color(0x224488) }),
    )
    earth.visible = false
    world.add(earth)
    new THREE.TextureLoader().load('/earth.webp', tex => {
      earth.material.map = tex
      earth.material.needsUpdate = true
      earth.visible = true
    })

    // Atmospheric Fresnel rim: thin bright halo on Earth's sunlit limb.
    // Only glows where the limb (viewDir·normal ≈ 0) AND sun hits (normal·sunDir > 0).
    const atmosphere = new THREE.Mesh(
      new THREE.SphereGeometry(1.025, 64, 64),
      new THREE.ShaderMaterial({
        uniforms: {
          sunDir:    { value: SUN_DIR.clone() },
          glowColor: { value: new THREE.Color(0x6aa9ff) },
          power:     { value: 4.5 },
          intensity: { value: 1.8 },
        },
        vertexShader: `
          varying vec3 vWorldNormal;
          varying vec3 vWorldPos;
          void main() {
            vec4 wp = modelMatrix * vec4(position, 1.0);
            vWorldPos = wp.xyz;
            vWorldNormal = normalize(mat3(modelMatrix) * normal);
            gl_Position = projectionMatrix * viewMatrix * wp;
          }
        `,
        fragmentShader: `
          uniform vec3 sunDir;
          uniform vec3 glowColor;
          uniform float power;
          uniform float intensity;
          varying vec3 vWorldNormal;
          varying vec3 vWorldPos;
          void main() {
            vec3 viewDir = normalize(cameraPosition - vWorldPos);
            float fresnel = pow(1.0 - max(dot(viewDir, vWorldNormal), 0.0), power);
            float sunMask = max(dot(vWorldNormal, normalize(sunDir)), 0.0);
            sunMask = pow(sunMask, 0.55);
            float a = fresnel * sunMask * intensity;
            gl_FragColor = vec4(glowColor * a, a);
          }
        `,
        transparent: true,
        blending: THREE.AdditiveBlending,
        side: THREE.FrontSide,
        depthWrite: false,
      }),
    )
    world.add(atmosphere)

const satGeo  = new THREE.SphereGeometry(0.022, 6, 6)
    const satMat  = new THREE.MeshBasicMaterial({ color: 0xddeeff })
    const lineMat = new THREE.LineBasicMaterial({ color: 0x1a3060, transparent: true, opacity: 0.18 })

    function orbitLineGeo(r) {
      const pts = []
      for (let i = 0; i <= 64; i++) {
        const t = (i / 64) * Math.PI * 2
        pts.push(new THREE.Vector3(r * Math.cos(t), 0, r * Math.sin(t)))
      }
      return new THREE.BufferGeometry().setFromPoints(pts)
    }

    const sats = Array.from({ length: 20 }, () => {
      const radius = 1.55 + Math.random() * 0.9
      const incl   = Math.random() * Math.PI * 0.85
      const raan   = Math.random() * Math.PI * 2
      const speed  = (0.004 + Math.random() * 0.008) * (Math.random() < 0.5 ? 1 : -1)
      const angle  = Math.random() * Math.PI * 2
      const pivot  = new THREE.Object3D()
      pivot.rotation.x = incl
      pivot.rotation.y = raan
      world.add(pivot)
      pivot.add(new THREE.LineLoop(orbitLineGeo(radius), lineMat))
      const sat = new THREE.Mesh(satGeo, satMat)
      sat.position.set(radius * Math.cos(angle), 0, radius * Math.sin(angle))
      pivot.add(sat)
      return { sat, radius, speed, angle }
    })

    function onResize() {
      camera.aspect = el.clientWidth / el.clientHeight
      camera.updateProjectionMatrix()
      renderer.setSize(el.clientWidth, el.clientHeight)
    }
    window.addEventListener('resize', onResize)

    // Continuous animation: the globe turns and the satellites orbit. The loop
    // also guarantees a repaint once the Earth texture finishes loading after mount.
    let raf
    function animate() {
      raf = requestAnimationFrame(animate)
      earth.rotation.y += 0.001
      sats.forEach(s => {
        s.angle += s.speed
        s.sat.position.x = s.radius * Math.cos(s.angle)
        s.sat.position.z = s.radius * Math.sin(s.angle)
      })
      renderer.render(scene, camera)
    }
    animate()

    return () => {
      cancelAnimationFrame(raf)
      window.removeEventListener('resize', onResize)
      if (earth.material.map) earth.material.map.dispose()
      scene.traverse(obj => {
        if (obj.geometry) obj.geometry.dispose()
        if (obj.material) {
          const mats = Array.isArray(obj.material) ? obj.material : [obj.material]
          mats.forEach(m => m.dispose())
        }
      })
      renderer.dispose()
      if (el.contains(renderer.domElement)) el.removeChild(renderer.domElement)
    }
  }, [])

  return <div ref={containerRef} className="absolute inset-0" />
}
