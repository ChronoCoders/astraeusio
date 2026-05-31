import { Link } from 'react-router-dom'
import Navbar from './Navbar.jsx'
import Footer from './Footer.jsx'

function Section({ title, children }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-zinc-100 text-base font-semibold">{title}</h2>
      <div className="flex flex-col gap-2 text-zinc-400 text-sm leading-relaxed">
        {children}
      </div>
    </section>
  )
}

export default function TermsPage({ onSignIn }) {
  return (
    <div className="min-h-screen bg-zinc-950 flex flex-col">
      <Navbar onSignIn={onSignIn} />

      <main className="flex-1 max-w-3xl mx-auto px-6 py-16 w-full">

        <div className="flex flex-col gap-2 mb-12">
          <span className="text-zinc-500 text-[10px] uppercase tracking-widest font-mono">Legal</span>
          <h1 className="text-2xl font-thin tracking-wide text-zinc-100">Terms of Service</h1>
          <p className="text-zinc-500 text-xs font-mono">Effective date: May 12, 2026 · Last updated: May 12, 2026</p>
        </div>

        <div className="flex flex-col gap-10">

          <Section title="Acceptance of Terms">
            <p>
              By accessing or using Astraeusio at astraeusio.com ("Service"), you agree to be bound by
              these Terms of Service ("Terms"). If you do not agree, do not use the Service. These Terms
              constitute a legally binding agreement between you and Astraeusio ("we," "us," or "our").
            </p>
            <p>
              We may update these Terms at any time. Continued use of the Service after changes are posted
              constitutes your acceptance of the revised Terms.
            </p>
          </Section>

          <Section title="Description of Service">
            <p>
              Astraeusio provides a real-time space weather monitoring dashboard and API. Data displayed
              is sourced from publicly available feeds operated by the National Oceanic and Atmospheric
              Administration (NOAA), the National Aeronautics and Space Administration (NASA), and other
              public data providers. We apply ML-based forecasting on top of this data.
            </p>
            <p className="text-yellow-400/80 border border-yellow-800/40 bg-yellow-950/20 rounded px-3 py-2">
              <span className="font-medium text-yellow-300">Important disclaimer:</span> Astraeusio is
              an informational tool only. Data and forecasts provided by this Service are not intended
              for use in safety-critical systems, aviation operations, power grid management, satellite
              operations, or any application where inaccurate data could cause harm or financial loss.
              Always rely on official NOAA Space Weather Prediction Center advisories for operational
              decisions.
            </p>
          </Section>

          <Section title="Account Registration">
            <p>To access certain features, you must create an account. You agree to:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li>Provide accurate and complete registration information</li>
              <li>Keep your password confidential and not share it with others</li>
              <li>Notify us immediately of any unauthorized use of your account</li>
              <li>Be at least 13 years of age</li>
            </ul>
            <p>You are responsible for all activity that occurs under your account.</p>
          </Section>

          <Section title="Acceptable Use">
            <p>You agree not to:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li>Use the Service for any unlawful purpose or in violation of any applicable laws</li>
              <li>Attempt to gain unauthorized access to our systems or other users' accounts</li>
              <li>Reverse-engineer, scrape, or otherwise extract data beyond your plan's API limits</li>
              <li>Use the Service to transmit malware, spam, or other malicious content</li>
              <li>Resell or redistribute raw API access without our written permission</li>
              <li>Circumvent, disable, or interfere with security features of the Service</li>
              <li>Impersonate any person or entity or misrepresent your affiliation</li>
            </ul>
            <p>We reserve the right to suspend or terminate accounts that violate these restrictions.</p>
          </Section>

          <Section title="API Usage and Rate Limits">
            <p>API access is subject to rate limits based on your plan tier:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li><span className="text-zinc-300 font-medium">Free</span> - limited requests per month, dashboard access only</li>
              <li><span className="text-zinc-300 font-medium">Starter</span> - increased limits, basic API access</li>
              <li><span className="text-zinc-300 font-medium">Pro</span> - higher limits, webhooks, full API access</li>
              <li><span className="text-zinc-300 font-medium">Enterprise</span> - custom limits, custom anomaly rules, priority support</li>
            </ul>
            <p>Exceeding your plan's rate limits will result in requests being rejected until your
              billing period resets. We reserve the right to adjust plan limits with reasonable notice.</p>
          </Section>

          <Section title="Intellectual Property">
            <p>
              The Astraeusio platform, including its source code, design, content, and documentation,
              is the proprietary work of Altug Tatlisu / ChronoCoders. All rights are reserved.
              The repository may be publicly visible for transparency, but no licence is granted to
              copy, modify, redistribute, or otherwise use the source code without prior written
              permission from the copyright holder.
            </p>
            <p>
              Space weather data displayed by the Service originates from NOAA, NASA, and other public
              data feeds and is not our intellectual property. We make no copyright claim over that data.
            </p>
            <p>
              The Astraeusio name, logo, and brand are our property. You may not use them without
              our written consent.
            </p>
          </Section>

          <Section title="Disclaimer of Warranties">
            <p>
              THE SERVICE IS PROVIDED "AS IS" AND "AS AVAILABLE" WITHOUT WARRANTIES OF ANY KIND,
              EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO WARRANTIES OF MERCHANTABILITY, FITNESS
              FOR A PARTICULAR PURPOSE, ACCURACY, OR NON-INFRINGEMENT.
            </p>
            <p>
              We do not warrant that the Service will be uninterrupted, error-free, or that data will
              be accurate or complete. Space weather data is subject to the accuracy of upstream NOAA
              and NASA feeds, which are outside our control.
            </p>
          </Section>

          <Section title="Limitation of Liability">
            <p>
              TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT SHALL ASTRAEUSIO, ITS
              OPERATORS, OR AFFILIATES BE LIABLE FOR ANY INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL,
              OR PUNITIVE DAMAGES - INCLUDING LOSS OF PROFITS, DATA, OR GOODWILL - ARISING FROM YOUR
              USE OF OR INABILITY TO USE THE SERVICE, EVEN IF WE HAVE BEEN ADVISED OF THE POSSIBILITY
              OF SUCH DAMAGES.
            </p>
            <p>
              OUR TOTAL LIABILITY TO YOU FOR ANY CLAIM ARISING FROM OR RELATED TO THE SERVICE SHALL
              NOT EXCEED THE GREATER OF (A) THE AMOUNT YOU PAID US IN THE 12 MONTHS PRECEDING THE
              CLAIM OR (B) $100 USD.
            </p>
          </Section>

          <Section title="Indemnification">
            <p>
              You agree to indemnify and hold harmless Astraeusio and its operators from and against
              any claims, damages, losses, or expenses (including reasonable attorneys' fees) arising
              from your use of the Service, your violation of these Terms, or your violation of any
              third-party rights.
            </p>
          </Section>

          <Section title="Termination">
            <p>
              You may delete your account at any time from the Settings page. Upon deletion, your data
              will be removed per our Privacy Policy.
            </p>
            <p>
              We may suspend or terminate your access to the Service immediately and without notice if
              we determine you have violated these Terms, engaged in fraudulent activity, or for any
              other reason at our sole discretion.
            </p>
          </Section>

          <Section title="Governing Law">
            <p>
              These Terms are governed by the laws of the United States. Any disputes arising from or
              relating to these Terms shall be resolved exclusively in the federal or state courts of
              competent jurisdiction. You consent to the personal jurisdiction of such courts.
            </p>
          </Section>

          <Section title="Entire Agreement">
            <p>
              These Terms, together with our <Link to="/privacy"
                className="text-zinc-300 hover:text-white underline transition-colors">Privacy Policy</Link>,
              constitute the entire agreement between you and Astraeusio regarding the Service and
              supersede all prior agreements or understandings.
            </p>
          </Section>

          <Section title="Contact">
            <p>Questions about these Terms? Contact us at:</p>
            <p>
              <a href="mailto:hello@astraeusio.com"
                className="text-zinc-300 hover:text-white underline transition-colors">
                hello@astraeusio.com
              </a>
            </p>
          </Section>

        </div>

        <div className="mt-12 pt-8 border-t border-zinc-800/60 flex gap-6">
          <Link to="/privacy" className="text-zinc-500 hover:text-zinc-300 text-xs transition-colors">
            Privacy Policy →
          </Link>
          <Link to="/" className="text-zinc-500 hover:text-zinc-300 text-xs transition-colors">
            ← Back to home
          </Link>
        </div>

      </main>

      <Footer />
    </div>
  )
}
