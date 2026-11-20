package bit.kedybaykedy.northbridge.transport

import android.content.SharedPreferences
import android.os.Build
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x500.X500NameBuilder
import org.bouncycastle.asn1.x509.BCStyle
import org.bouncycastle.asn1.x509.ExtendedKeyUsage
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.KeyPurposeId
import org.bouncycastle.asn1.x509.KeyUsage
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter
import org.bouncycastle.cert.jcajce.JcaX509ExtensionUtils
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import java.io.ByteArrayInputStream
import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.Security
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Base64
import java.util.Date

data class PhoneIdentity(
    val alias: String,
    val certPem: String,
    val keyPem: String,
    val fingerprint: String,
    val cert: X509Certificate,
    val privateKey: java.security.PrivateKey,
)

object IdentityStore {

    private const val KEY_ALIAS = "identity_alias"
    private const val KEY_CERT = "identity_cert"
    private const val KEY_KEY = "identity_key"
    private const val KEY_FINGERPRINT = "identity_fingerprint"

    fun loadOrGenerate(prefs: SharedPreferences): PhoneIdentity {
        val certPem = prefs.getString(KEY_CERT, null)
        val keyPem = prefs.getString(KEY_KEY, null)
        val fingerprint = prefs.getString(KEY_FINGERPRINT, null)
        if (certPem != null && keyPem != null && fingerprint != null) {
            val alias = prefs.getString(KEY_ALIAS, null) ?: defaultAlias(fingerprint)
            return parse(alias, certPem, keyPem, fingerprint)
        }
        val alias = prefs.getString(KEY_ALIAS, null) ?: defaultAlias()
        val identity = generate(alias)
        prefs.edit()
            .putString(KEY_ALIAS, identity.alias)
            .putString(KEY_CERT, identity.certPem)
            .putString(KEY_KEY, identity.keyPem)
            .putString(KEY_FINGERPRINT, identity.fingerprint)
            .apply()
        return identity
    }

    private fun defaultAlias(fingerprint: String? = null): String {
        val model = Build.MODEL?.trim().orEmpty()
        if (model.isNotEmpty()) return model
        return if (fingerprint != null) {
            "phone-${fingerprint.take(8).lowercase()}"
        } else {
            "northbridge-phone"
        }
    }

    private fun generate(alias: String): PhoneIdentity {
        if (Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
            Security.addProvider(BouncyCastleProvider())
        }
        val keyPairGenerator = KeyPairGenerator.getInstance("RSA")
        keyPairGenerator.initialize(2048, SecureRandom())
        val keyPair = keyPairGenerator.generateKeyPair()
        val subject = X500NameBuilder().addRDN(BCStyle.CN, alias).build()
        val serial = BigInteger(128, SecureRandom())
        val notBefore = Date(946_684_800_000L)
        val notAfter = Date(4_102_444_800_000L)
        val builder = JcaX509v3CertificateBuilder(
            subject,
            serial,
            notBefore,
            notAfter,
            subject,
            keyPair.public,
        )
        val extensions = JcaX509ExtensionUtils()
        builder.addExtension(
            Extension.subjectKeyIdentifier,
            false,
            extensions.createSubjectKeyIdentifier(keyPair.public),
        )
        builder.addExtension(
            Extension.keyUsage,
            true,
            KeyUsage(KeyUsage.digitalSignature or KeyUsage.keyEncipherment),
        )
        builder.addExtension(
            Extension.extendedKeyUsage,
            false,
            ExtendedKeyUsage(KeyPurposeId.id_kp_clientAuth),
        )
        val signer = JcaContentSignerBuilder("SHA256withRSA")
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .build(keyPair.private)
        val certificate = JcaX509CertificateConverter().getCertificate(builder.build(signer))
        val certPem = pemEncode("CERTIFICATE", certificate.encoded)
        val keyPem = pemEncode("PRIVATE KEY", keyPair.private.encoded)
        return PhoneIdentity(
            alias = alias,
            certPem = certPem,
            keyPem = keyPem,
            fingerprint = sha256Upper(certificate.encoded),
            cert = certificate,
            privateKey = keyPair.private,
        )
    }

    private fun parse(alias: String, certPem: String, keyPem: String, fingerprint: String): PhoneIdentity {
        val certificateFactory = CertificateFactory.getInstance("X.509")
        val cert = certificateFactory
            .generateCertificate(ByteArrayInputStream(pemDecode(certPem))) as X509Certificate
        val privateKey = KeyFactory.getInstance("RSA")
            .generatePrivate(PKCS8EncodedKeySpec(pemDecode(keyPem)))
        return PhoneIdentity(
            alias = alias,
            certPem = certPem,
            keyPem = keyPem,
            fingerprint = fingerprint,
            cert = cert,
            privateKey = privateKey,
        )
    }

    private fun pemEncode(tag: String, der: ByteArray): String {
        val base64 = Base64.getEncoder().encodeToString(der)
        val lines = base64.chunked(64)
        return buildString {
            append("-----BEGIN ").append(tag).append("-----\n")
            lines.forEach { append(it).append('\n') }
            append("-----END ").append(tag).append("-----\n")
        }
    }

    private fun pemDecode(pem: String): ByteArray {
        val body = pem
            .lineSequence()
            .filter { !it.startsWith("-----") && it.isNotBlank() }
            .joinToString("")
        return Base64.getDecoder().decode(body)
    }
}

fun sha256Upper(data: ByteArray): String {
    val digest = MessageDigest.getInstance("SHA-256").digest(data)
    val hex = "0123456789ABCDEF"
    val builder = StringBuilder(digest.size * 2)
    for (byte in digest) {
        val value = byte.toInt() and 0xFF
        builder.append(hex[value shr 4])
        builder.append(hex[value and 0xF])
    }
    return builder.toString()
}
