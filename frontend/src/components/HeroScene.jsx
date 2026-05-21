import { useEffect, useRef } from 'react'
import * as THREE from 'three'

export default function HeroScene() {
  const containerRef = useRef(null)

  useEffect(() => {
    const el = containerRef.current
    if (!el) return

    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    const renderer = new THREE.WebGLRenderer({ antialias: true })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    renderer.setSize(el.clientWidth, el.clientHeight)
    renderer.setClearColor(0x09090b, 1)
    el.appendChild(renderer.domElement)

    const scene  = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(50, el.clientWidth / el.clientHeight, 0.1, 100)
    camera.position.set(0, 0.8, 4.2)
    camera.lookAt(0, 0, 0)

    scene.add(new THREE.AmbientLight(0x334466, 1.2))
    const sun = new THREE.DirectionalLight(0x6699cc, 1.5)
    sun.position.set(5, 3, 4)
    scene.add(sun)
    const fill = new THREE.DirectionalLight(0x4477bb, 0.8)
    fill.position.set(-5, 1, 2)
    scene.add(fill)
    const back = new THREE.DirectionalLight(0x112244, 0.4)
    back.position.set(-3, -1, -4)
    scene.add(back)

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

    let raf
    if (reduced) {
      renderer.render(scene, camera)
    } else {
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
    }

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
