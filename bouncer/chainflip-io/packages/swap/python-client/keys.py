from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

# generate a new key pair
private_key = ed25519.Ed25519PrivateKey.generate()
public_key = private_key.public_key()

# serialize the private key
private_key_bytes = private_key.private_bytes(
    encoding=serialization.Encoding.PEM,
    format=serialization.PrivateFormat.PKCS8,
    encryption_algorithm=serialization.NoEncryption(),
)

# serialize the public key
public_key_bytes = public_key.public_bytes(
    encoding=serialization.Encoding.PEM,
    format=serialization.PublicFormat.SubjectPublicKeyInfo,
)

# print the private key
print(private_key_bytes.decode("utf-8"))

# print the public key
print(public_key_bytes.decode("utf-8"))
