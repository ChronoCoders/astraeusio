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

export default function PrivacyPage({ onSignIn }) {
  return (
    <div className="min-h-screen bg-zinc-950 flex flex-col">
      <Navbar onSignIn={onSignIn} />

      <main className="flex-1 max-w-3xl mx-auto px-6 py-16 w-full">

        <div className="flex flex-col gap-2 mb-12">
          <span className="text-zinc-500 text-[10px] uppercase tracking-widest font-mono">Legal</span>
          <h1 className="text-2xl font-thin tracking-wide text-zinc-100">Privacy Policy</h1>
          <p className="text-zinc-500 text-xs font-mono">Effective date: May 12, 2026 · Last updated: May 12, 2026</p>
        </div>

        <div className="flex flex-col gap-10">

          <Section title="Overview">
            <p>
              Astraeusio ("we," "us," or "our") operates astraeusio.com. This Privacy Policy explains what
              personal information we collect, how we use it, and your rights regarding that information.
              By using our service, you agree to the collection and use of information as described here.
            </p>
            <p>
              We do not sell, rent, or trade your personal information to third parties.
            </p>
          </Section>

          <Section title="Information We Collect">
            <p><span className="text-zinc-300 font-medium">Account data.</span> When you register, we collect your email address and a
              bcrypt-hashed representation of your password. We never store your password in plain text.</p>
            <p><span className="text-zinc-300 font-medium">Usage data.</span> We track API request counts per billing period to enforce
              plan limits. We do not log the content of your requests beyond what is necessary for
              rate-limiting and abuse prevention.</p>
            <p><span className="text-zinc-300 font-medium">Preferences you provide.</span> If you configure email alert thresholds,
              webhooks, or custom anomaly rules, those settings are stored and associated with your account.</p>
            <p><span className="text-zinc-300 font-medium">Two-factor authentication.</span> If you enable TOTP 2FA, we store an encrypted
              TOTP secret associated with your account. We do not have access to the codes your
              authenticator app generates.</p>
            <p><span className="text-zinc-300 font-medium">Server logs.</span> Our server software may log IP addresses, request paths,
              and timestamps for security and debugging purposes. Logs are retained for up to 30 days.</p>
            <p><span className="text-zinc-300 font-medium">Data we do not collect.</span> We do not use tracking cookies, third-party
              analytics, advertising pixels, or browser fingerprinting.</p>
          </Section>

          <Section title="How We Use Your Information">
            <p>We use the information we collect to:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li>Create and authenticate your account</li>
              <li>Deliver the service, including real-time space weather data and ML forecasts</li>
              <li>Send transactional emails (email verification, password changes, alert notifications)
                that you have explicitly configured</li>
              <li>Enforce plan-based rate limits</li>
              <li>Investigate and prevent abuse, fraud, or security incidents</li>
              <li>Improve the reliability and performance of our infrastructure</li>
            </ul>
            <p>We do not use your information for advertising or behavioral profiling.</p>
          </Section>

          <Section title="Third-Party Service Providers">
            <p>We share limited data with the following processors solely to operate the service:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li><span className="text-zinc-300 font-medium">Resend (resend.com)</span> - transactional email delivery. Your email address is
                transmitted to Resend when we send you a verification or alert email. Resend's privacy
                policy governs their handling of that data.</li>
              <li><span className="text-zinc-300 font-medium">Cloudflare</span> - DNS resolution and TLS termination. Cloudflare may process
                request metadata in transit. Their privacy policy governs that processing.</li>
            </ul>
            <p>Space weather data displayed in the service is sourced from NOAA and NASA public APIs.
              No user data is transmitted to those agencies.</p>
          </Section>

          <Section title="Data Retention">
            <p>We retain account data for as long as your account is active. If you delete your account,
              your email, hashed password, API keys, alert settings, and usage records are permanently
              deleted within 30 days.</p>
            <p>Server logs are retained for up to 30 days, after which they are automatically deleted.</p>
          </Section>

          <Section title="Your Rights (CCPA and General)">
            <p>Regardless of your location, you have the right to:</p>
            <ul className="list-disc list-inside flex flex-col gap-1 pl-2">
              <li><span className="text-zinc-300 font-medium">Access</span> - request a copy of the personal information we hold about you.</li>
              <li><span className="text-zinc-300 font-medium">Deletion</span> - request that we delete your personal information. You can do
                this at any time from the Settings page, or by contacting us.</li>
              <li><span className="text-zinc-300 font-medium">Correction</span> - request correction of inaccurate data.</li>
              <li><span className="text-zinc-300 font-medium">Opt-out of email communications</span> - you can disable email alerts from the
                Settings page at any time.</li>
            </ul>
            <p>California residents have additional rights under the California Consumer Privacy Act (CCPA),
              including the right to know what personal information is sold or disclosed (we do neither)
              and the right to non-discrimination for exercising privacy rights.</p>
            <p>To exercise any of these rights, email us at <a href="mailto:contact@chronocoder.dev"
              className="text-zinc-300 hover:text-white underline transition-colors">contact@chronocoder.dev</a>.</p>
          </Section>

          <Section title="Children's Privacy">
            <p>Astraeusio is not directed to children under the age of 13. We do not knowingly collect
              personal information from children under 13. If you believe a child under 13 has provided
              us with personal information, please contact us and we will delete it promptly.</p>
          </Section>

          <Section title="Security">
            <p>We protect your data using industry-standard practices: passwords are hashed with bcrypt,
              all data in transit is encrypted via TLS, and access to production systems is restricted
              to authorized personnel only.</p>
            <p>No method of transmission over the internet is 100% secure. While we strive to protect
              your information, we cannot guarantee absolute security.</p>
          </Section>

          <Section title="Changes to This Policy">
            <p>We may update this Privacy Policy from time to time. If we make material changes, we will
              update the "Last updated" date at the top of this page. Continued use of the service after
              changes constitutes acceptance of the updated policy.</p>
          </Section>

          <Section title="Contact">
            <p>If you have questions or concerns about this Privacy Policy or our data practices, contact us at:</p>
            <p>
              <a href="mailto:contact@chronocoder.dev"
                className="text-zinc-300 hover:text-white underline transition-colors">
                contact@chronocoder.dev
              </a>
            </p>
          </Section>

        </div>

        <div className="mt-12 pt-8 border-t border-zinc-800/60 flex gap-6">
          <Link to="/terms" className="text-zinc-500 hover:text-zinc-300 text-xs transition-colors">
            Terms of Service →
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
